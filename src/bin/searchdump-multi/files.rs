//! File naming (RS and Node layouts), split parts, record counts and the meta directory.

use crate::*;

/// Subdirectory of the dump root for cluster-scoped files. Index names cannot start with `_`,
/// so it never collides with an index directory.
pub const CLUSTER_DIR: &str = "_cluster";

pub fn item_file(index: &str, kind: &str, compression: Compression) -> String {
    if kind == "data" {
        format!("{index}.data.{}", compression.suffix())
    } else {
        format!("{index}.{kind}.json")
    }
}

/// File names in Node multielasticdump's layout: `INDEX.json[.gz]` for data and
/// `INDEX.TYPE.json[.gz]` for the rest, every file gzipped or none.
pub fn node_item_file(index: &str, kind: &str, compression: Compression) -> String {
    let gz = if compression == Compression::Gzip {
        ".gz"
    } else {
        ""
    };
    if kind == "data" {
        format!("{index}.json{gz}")
    } else {
        format!("{index}.{kind}.json{gz}")
    }
}

/// The file for one item in either naming.
pub fn item_file_for(node: bool, index: &str, kind: &str, compression: Compression) -> String {
    if node {
        node_item_file(index, kind, compression)
    } else {
        item_file(index, kind, compression)
    }
}

/// Name suffixes of the non-data files of an index, in either naming (`.index` is Node's
/// `--type=index` file).
pub(crate) const META_SUFFIXES: &[&str] = &[
    "settings",
    "analyzer",
    "mapping",
    "alias",
    "template",
    "index",
    "datastream",
];

/// What a data-file name says: the index, the codec, and the part number of a split dump.
/// searchdump names: `INDEX.data.json[.gz|.zst]`, `INDEX.data.split-N.json[...]`;
/// Node names: `INDEX.json[.gz]`, `INDEX.split-N.json[.gz]`.
pub fn parse_data_file(name: &str) -> Option<(String, Compression, Option<u64>)> {
    if name == MANIFEST {
        return None;
    }
    let (stem, c) = if let Some(s) = name.strip_suffix(".json.gz") {
        (s, Compression::Gzip)
    } else if let Some(s) = name.strip_suffix(".json.zst") {
        (s, Compression::Zstd3)
    } else if let Some(s) = name.strip_suffix(".json") {
        (s, Compression::None)
    } else {
        return None;
    };
    if let Some(index) = stem.strip_suffix(".data") {
        return (!index.is_empty()).then(|| (index.to_string(), c, None));
    }
    if let Some((head, n)) = stem.rsplit_once(".split-") {
        if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) {
            let head = head.strip_suffix(".data").unwrap_or(head);
            let part: u64 = n.parse().ok()?;
            return (!head.is_empty()).then(|| (head.to_string(), c, Some(part)));
        }
        return None;
    }
    if stem.is_empty()
        || META_SUFFIXES
            .iter()
            .any(|k| stem.ends_with(&format!(".{k}")))
    {
        return None;
    }
    Some((stem.to_string(), c, None))
}

/// Whether a data-file name is in searchdump naming (`.data.` before the extension).
pub(crate) fn rs_named(name: &str) -> bool {
    name.contains(".data.json") || name.contains(".data.split-")
}

/// An index's non-data file in either form (`INDEX.KIND.json`, then `.json.gz`), if it is
/// there and not empty; the flag says it is gzipped.
pub fn meta_file(dir: &Path, index: &str, kind: &str) -> Option<(PathBuf, bool)> {
    [("json", false), ("json.gz", true)]
        .into_iter()
        .map(|(ext, gz)| (dir.join(format!("{index}.{kind}.{ext}")), gz))
        .find(|(p, _)| nonempty(p))
}

/// A text file, gunzipped when its name ends in `.gz`.
pub(crate) fn read_text(p: &Path) -> Result<String> {
    let f = fs::File::open(p).with_context(|| format!("open {}", p.display()))?;
    let mut s = String::new();
    if p.extension().is_some_and(|e| e == "gz") {
        flate2::read::MultiGzDecoder::new(f).read_to_string(&mut s)
    } else {
        std::io::BufReader::new(f).read_to_string(&mut s)
    }
    .with_context(|| format!("read {}", p.display()))?;
    Ok(s)
}

/// A split dump's part files for one index in `dir`, in part order. Errors on a gap or on parts
/// in more than one codec.
pub fn data_parts(dir: &Path, index: &str) -> Result<Vec<PathBuf>> {
    let mut parts: Vec<(u64, Compression, PathBuf)> = Vec::new();
    for ent in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let ent = ent?;
        let name = ent.file_name().to_string_lossy().into_owned();
        if let Some((i, c, Some(n))) = parse_data_file(&name)
            && i == index
            && ent.file_type()?.is_file()
        {
            parts.push((n, c, ent.path()));
        }
    }
    parts.sort_by_key(|p| p.0);
    if let Some(w) = parts.windows(2).find(|w| w[0].0 == w[1].0) {
        bail!(
            "{index}: data part {} twice ({} and {})",
            w[0].0,
            w[0].2.display(),
            w[1].2.display()
        );
    }
    if parts.windows(2).any(|w| w[0].1.suffix() != w[1].1.suffix()) {
        bail!(
            "{index}: data parts in more than one codec in {}",
            dir.display()
        );
    }
    for (want, (n, _, _)) in parts.iter().enumerate() {
        if *n != want as u64 {
            bail!("{index}: data part {want} is missing in {}", dir.display());
        }
    }
    Ok(parts.into_iter().map(|p| p.2).collect())
}

pub(crate) fn is_part(p: &Path) -> bool {
    parse_data_file(&p.file_name().unwrap_or_default().to_string_lossy())
        .is_some_and(|d| d.2.is_some())
}

// ----------------------------------------------------------------------------- data files

/// Records (lines) in a data file, decompressing by its name.
pub fn count_records(path: &Path) -> Result<u64> {
    let f = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut r: Box<dyn Read> = match parse_data_file(&name).map(|d| d.1) {
        Some(Compression::Gzip) => Box::new(flate2::read::MultiGzDecoder::new(f)),
        Some(Compression::Zstd1 | Compression::Zstd3) => {
            Box::new(zstd::stream::read::Decoder::new(f)?)
        }
        _ => Box::new(f),
    };
    let mut buf = vec![0u8; 1 << 20];
    let mut n = 0u64;
    loop {
        let k = r
            .read(&mut buf)
            .with_context(|| format!("read {}", path.display()))?;
        if k == 0 {
            return Ok(n);
        }
        n += memchr::memchr_iter(b'\n', &buf[..k]).count() as u64;
    }
}

/// elasticdump writes settings and mappings as JSON, sometimes as a JSON string of JSON.
pub fn mappings_body(text: &str) -> Result<serde_json::Value> {
    let mut doc: serde_json::Value = serde_json::from_str(text.trim())?;
    if let serde_json::Value::String(s) = &doc {
        doc = serde_json::from_str(s)?;
    }
    let obj = doc
        .as_object()
        .filter(|o| !o.is_empty())
        .ok_or_else(|| anyhow!("mapping dump is empty"))?;
    let (_, body) = obj.iter().next().unwrap();
    Ok(body
        .get("mappings")
        .cloned()
        .unwrap_or_else(|| body.clone()))
}

/// Remove this index's earlier data files (single or split) before a new data dump, so stale
/// parts from a larger earlier dump are not counted or loaded. Only files named exactly as
/// this tool names them are touched.
pub(crate) fn remove_data_files(dir: &Path, index: &str) -> Result<()> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Ok(());
    };
    for ent in rd {
        let ent = ent?;
        let name = ent.file_name().to_string_lossy().into_owned();
        if parse_data_file(&name).is_some_and(|d| d.0 == index) && ent.file_type()?.is_file() {
            fs::remove_file(ent.path())
                .with_context(|| format!("remove stale {}", ent.path().display()))?;
        }
    }
    Ok(())
}

pub(crate) fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(crate) fn nonempty(p: &Path) -> bool {
    fs::metadata(p)
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false)
}

/// Where the manifest and `_cluster/` go: --metaDir; else for a dump the directory itself, or
/// `DIR.meta` beside it with --nodeCompat (Node takes every entry in DIR for an index); for a
/// load the directory when it holds a manifest or `_cluster/`, else `DIR.meta` when that exists.
pub fn meta_dir(args: &Args) -> PathBuf {
    if let Some(d) = &args.meta_dir {
        return d.clone();
    }
    let dir = match args.direction {
        Direction::Dump => &args.output,
        Direction::Load | Direction::Copy => &args.input,
    };
    let root = PathBuf::from(dir);
    let beside = PathBuf::from(format!("{}.meta", dir.trim_end_matches('/')));
    match args.direction {
        Direction::Dump if args.node_compat => beside,
        Direction::Dump => root,
        Direction::Load | Direction::Copy => {
            if root.join(MANIFEST).exists() || root.join(CLUSTER_DIR).is_dir() || !beside.is_dir() {
                root
            } else {
                beside
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mappings_body_shapes() {
        let plain = r#"{"idx":{"mappings":{"properties":{"a":{"type":"keyword"}}}}}"#;
        assert_eq!(
            mappings_body(plain).unwrap()["properties"]["a"]["type"],
            "keyword"
        );
        let stringified = serde_json::to_string(plain).unwrap();
        assert_eq!(
            mappings_body(&stringified).unwrap()["properties"]["a"]["type"],
            "keyword"
        );
        assert!(mappings_body("{}").is_err());
    }

    #[test]
    fn record_counts_all_codecs() {
        let d = tempfile::tempdir().unwrap();
        let body = b"{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n";
        let plain = d.path().join("i.data.json");
        fs::write(&plain, body).unwrap();
        let gz = d.path().join("i.data.json.gz");
        let mut e = flate2::write::GzEncoder::new(
            fs::File::create(&gz).unwrap(),
            flate2::Compression::default(),
        );
        e.write_all(body).unwrap();
        e.finish().unwrap();
        let zst = d.path().join("j.data.json.zst");
        fs::write(&zst, zstd::stream::encode_all(&body[..], 3).unwrap()).unwrap();
        for p in [&plain, &gz, &zst] {
            assert_eq!(count_records(p).unwrap(), 3, "{}", p.display());
        }
    }

    #[test]
    fn node_names_and_meta_dir() {
        assert_eq!(node_item_file("i", "data", Compression::Gzip), "i.json.gz");
        assert_eq!(
            node_item_file("i", "settings", Compression::Gzip),
            "i.settings.json.gz"
        );
        assert_eq!(
            node_item_file("i", "template", Compression::None),
            "i.template.json"
        );
        assert_eq!(
            parse_data_file("i.json.gz"),
            Some(("i".into(), Compression::Gzip, None))
        );
        assert_eq!(
            parse_data_file("i.split-3.json.gz"),
            Some(("i".into(), Compression::Gzip, Some(3)))
        );
        for meta in [
            "i.settings.json.gz",
            "i.template.json",
            "i.index.json.gz",
            "multielasticdump.json",
            ".json",
        ] {
            assert_eq!(parse_data_file(meta), None, "{meta}");
        }
        // a Node-named dump directory, as Node 6.x writes it
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        for f in [
            "a.json.gz",
            "a.settings.json.gz",
            "a.mapping.json.gz",
            "a.template.json.gz",
            "notes.json",
        ] {
            fs::write(r.join(f), "x").unwrap();
        }
        let found = discover(r, r).unwrap();
        assert_eq!(found.len(), 1, "a lone notes.json is not an index");
        assert_eq!(found[0].index, "a");
        assert!(found[0].data[0].ends_with("a.json.gz"));
        assert_eq!(
            meta_file(r, "a", "settings").map(|m| m.1),
            Some(true),
            "gzipped metadata found"
        );
        // the same part number in both namings is an error, not a silent pick
        fs::write(r.join("b.split-0.json"), "{}\n").unwrap();
        fs::write(r.join("b.data.split-0.json"), "{}\n").unwrap();
        assert!(data_parts(r, "b").is_err());
        // --metaDir defaults
        let mut a = Args::try_parse_from([
            "m",
            "--input=http://h:9200",
            "--output=/backup/x/",
            "--nodeCompat",
        ])
        .unwrap();
        assert_eq!(meta_dir(&a), PathBuf::from("/backup/x.meta"));
        a.node_compat = false;
        assert_eq!(meta_dir(&a), PathBuf::from("/backup/x/"));
        let load = Args::try_parse_from([
            "m",
            "--direction=load",
            &format!("--input={}", r.display()),
            "--output=http://h:9200",
        ])
        .unwrap();
        assert_eq!(
            meta_dir(&load),
            r.to_path_buf(),
            "no DIR.meta: the directory"
        );
        fs::create_dir_all(format!("{}.meta", r.display())).unwrap();
        assert_eq!(
            meta_dir(&load),
            PathBuf::from(format!("{}.meta", r.display()))
        );
        fs::remove_dir_all(format!("{}.meta", r.display())).unwrap();
    }
}
