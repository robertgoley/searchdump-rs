//! Finding loadable indices and parts in a dump directory.

use crate::*;

// ----------------------------------------------------------------------------- discovery (load)

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub index: String,
    pub dir: PathBuf,
    /// Data files in load order: one file, or the parts of a split dump; empty for none.
    pub data: Vec<PathBuf>,
    pub records_hint: Option<u64>,
    /// A data stream (written with `--bulkAction=create` to the stream name).
    pub datastream: bool,
}

/// Indices in a dump directory: from the manifest (in `meta`) when there is one, else every
/// `INDEX.settings.json` / `INDEX.mapping.json` / `INDEX.data*.json*` / `INDEX.datastream.json`
/// up to two levels down (the `_cluster` directory excepted). Node multielasticdump names
/// (`INDEX.json.gz`, `INDEX.settings.json.gz`, `INDEX.split-N.json.gz`) are found too; a
/// Node-named data file counts only next to a settings or mapping file of the same index.
pub fn discover(root: &Path, meta: &Path) -> Result<Vec<Found>> {
    let mut out: BTreeMap<String, Found> = BTreeMap::new();
    let mut has_meta = std::collections::BTreeSet::new();
    if let Some(m) = Manifest::load(meta)? {
        for (name, e) in &m.indices {
            if e.status != "done" {
                eprintln!(
                    "note: {name} is recorded as {} in the manifest; skipping it",
                    e.status
                );
                continue;
            }
            check_index_name(name)?;
            let dir = root.join(&e.dir);
            let data: Vec<PathBuf> = if e.data_parts.is_empty() {
                e.data_file
                    .iter()
                    .map(|f| dir.join(f))
                    .filter(|p| p.is_file())
                    .collect()
            } else {
                let parts: Vec<PathBuf> = e.data_parts.iter().map(|f| dir.join(f)).collect();
                if let Some(p) = parts.iter().find(|p| !p.is_file()) {
                    bail!("{name}: data part {} is missing", p.display());
                }
                parts
            };
            out.insert(
                name.clone(),
                Found {
                    index: name.clone(),
                    dir,
                    data,
                    records_hint: e.records,
                    datastream: e.kind.as_deref() == Some("datastream"),
                },
            );
        }
        return Ok(out.into_values().collect());
    }
    let mut dirs = vec![(root.to_path_buf(), 0)];
    while let Some((d, depth)) = dirs.pop() {
        for ent in fs::read_dir(&d).with_context(|| format!("read {}", d.display()))? {
            let ent = ent?;
            let path = ent.path();
            let name = ent.file_name().to_string_lossy().into_owned();
            if ent.file_type()?.is_dir() {
                if depth < 2 && !(depth == 0 && name == CLUSTER_DIR) {
                    dirs.push((path, depth + 1));
                }
                continue;
            }
            let data = parse_data_file(&name);
            let marker = [
                "settings",
                "analyzer",
                "mapping",
                "alias",
                "template",
                "datastream",
            ]
            .iter()
            .flat_map(|k| [format!(".{k}.json"), format!(".{k}.json.gz")])
            .find_map(|suffix| name.strip_suffix(&suffix).map(|i| (i.to_string(), suffix)));
            let index = marker
                .as_ref()
                .map(|m| m.0.clone())
                .or_else(|| data.as_ref().map(|d| d.0.clone()));
            let Some(index) = index else { continue };
            if check_index_name(&index).is_err() {
                continue;
            }
            let key = format!("{}\0{index}", d.display());
            if marker
                .as_ref()
                .is_some_and(|m| m.1.starts_with(".settings.") || m.1.starts_with(".mapping."))
            {
                has_meta.insert(key.clone());
            }
            let entry = out.entry(key).or_insert_with(|| Found {
                index: index.clone(),
                dir: d.clone(),
                data: Vec::new(),
                records_hint: None,
                datastream: false,
            });
            if name.ends_with(".datastream.json") {
                entry.datastream = true;
            }
            if let Some((_, _, part)) = data
                && (fs::metadata(&path)?.len() > 0 || part.is_some())
            {
                entry.data.push(path);
            }
        }
    }
    // A Node-style `NAME.json` alone is just some JSON file, not an index.
    out.retain(|k, f| {
        has_meta.contains(k)
            || f.datastream
            || f.data.is_empty()
            || f.data.iter().any(|p| rs_named(&file_name(p)))
    });
    // Settle each index's data files: one single file, or a gap-free run of parts.
    for f in out.values_mut() {
        let (parts, singles): (Vec<PathBuf>, Vec<PathBuf>) = std::mem::take(&mut f.data)
            .into_iter()
            .partition(|p| is_part(p));
        match (singles.len(), parts.is_empty()) {
            (0, _) => {
                f.data = if parts.is_empty() {
                    parts
                } else {
                    data_parts(&f.dir, &f.index)?
                }
            }
            (1, true) => f.data = singles,
            (1, false) => bail!(
                "{}: both a single data file ({}) and split parts",
                f.index,
                singles[0].display()
            ),
            _ => bail!(
                "{}: more than one data file ({} and {})",
                f.index,
                singles[0].display(),
                singles[1].display()
            ),
        }
    }
    let mut seen: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut found: Vec<Found> = out.into_values().collect();
    found.sort_by(|a, b| a.index.cmp(&b.index));
    for f in &found {
        if let Some(prev) = seen.insert(f.index.clone(), f.dir.clone()) {
            bail!(
                "index {} appears in both {} and {}",
                f.index,
                prev.display(),
                f.dir.display()
            );
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_layouts_and_manifest() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        // oslogs.py flat files
        for f in ["a.settings.json", "a.mapping.json"] {
            fs::write(r.join(f), "{}").unwrap();
        }
        fs::write(r.join("a.data.json.gz"), b"x").unwrap();
        // grouped
        fs::create_dir_all(r.join("2026.10")).unwrap();
        for f in ["b.settings.json", "b.mapping.json", "b.data.json.zst"] {
            fs::write(r.join("2026.10").join(f), "{}").unwrap();
        }
        fs::write(r.join("notes.txt"), "x").unwrap();
        let found = discover(r, r).unwrap();
        let names: Vec<_> = found.iter().map(|f| f.index.as_str()).collect();
        assert_eq!(names, vec!["a", "b"]);
        assert_eq!(found[1].dir, r.join("2026.10"));
        assert!(found[1].data[0].ends_with("b.data.json.zst"));
        // duplicate across directories is an error
        fs::create_dir_all(r.join("dup")).unwrap();
        fs::write(r.join("dup").join("a.settings.json"), "{}").unwrap();
        assert!(discover(r, r).is_err());
        fs::remove_dir_all(r.join("dup")).unwrap();
        // two data files for one index is an error
        fs::write(r.join("a.data.json.zst"), b"x").unwrap();
        assert!(discover(r, r).is_err());
        fs::remove_file(r.join("a.data.json.zst")).unwrap();
        // a manifest wins and failed entries are skipped
        let mut m = Manifest {
            tool: "searchdump-multi".into(),
            version: "t".into(),
            ..Default::default()
        };
        m.indices.insert(
            "a".into(),
            IndexEntry {
                dir: "".into(),
                types: vec!["data".into()],
                data_file: Some("a.data.json.gz".into()),
                records: Some(1),
                status: "done".into(),
                ..Default::default()
            },
        );
        m.indices.insert(
            "c".into(),
            IndexEntry {
                status: "failed".into(),
                ..Default::default()
            },
        );
        m.save(r).unwrap();
        let found = discover(r, r).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].index, "a");
        assert_eq!(found[0].records_hint, Some(1));
    }

    #[test]
    fn split_part_names_and_discovery() {
        assert_eq!(
            parse_data_file("i.data.split-12.json.gz"),
            Some(("i".into(), Compression::Gzip, Some(12)))
        );
        assert_eq!(
            parse_data_file("i.x.data.split-0.json"),
            Some(("i.x".into(), Compression::None, Some(0)))
        );
        assert_eq!(
            parse_data_file("i.data.json.zst"),
            Some(("i".into(), Compression::Zstd3, None))
        );
        assert_eq!(parse_data_file("i.data.split-x.json"), None);
        assert_eq!(parse_data_file("i.mapping.json"), None);
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        for n in [0, 1, 2, 10] {
            fs::write(r.join(format!("p.data.split-{n}.json")), "{}\n").unwrap();
        }
        assert!(data_parts(r, "p").is_err(), "gap 3..9 must be an error");
        for n in 3..10 {
            fs::write(r.join(format!("p.data.split-{n}.json")), "{}\n").unwrap();
        }
        let parts = data_parts(r, "p").unwrap();
        assert_eq!(parts.len(), 11);
        assert!(
            parts[10].ends_with("p.data.split-10.json"),
            "numeric, not lexical, order"
        );
        fs::write(r.join("p.settings.json"), "{}").unwrap();
        // cluster-scoped files are not indices; a data stream marker is recognised
        fs::create_dir_all(r.join(CLUSTER_DIR)).unwrap();
        fs::write(r.join(CLUSTER_DIR).join("template.json"), "{}").unwrap();
        fs::write(r.join("web.datastream.json"), "{}").unwrap();
        fs::write(r.join("web.data.json.gz"), "x").unwrap();
        let found = discover(r, r).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].index, "p");
        assert_eq!(found[0].data.len(), 11);
        assert!(!found[0].datastream);
        assert_eq!(found[1].index, "web");
        assert!(found[1].datastream);
        // a single data file next to parts is ambiguous
        fs::write(r.join("p.data.json"), "{}\n").unwrap();
        assert!(discover(r, r).is_err());
        fs::remove_file(r.join("p.data.json")).unwrap();
        // parts in two codecs are an error
        fs::write(r.join("p.data.split-11.json.gz"), "x").unwrap();
        assert!(discover(r, r).is_err());
        fs::remove_file(r.join("p.data.split-11.json.gz")).unwrap();
        // stale-file cleanup only touches this index's data files
        remove_data_files(r, "p").unwrap();
        assert!(data_parts(r, "p").unwrap().is_empty());
        assert!(r.join("p.settings.json").exists());
        assert!(r.join("web.data.json.gz").exists());
    }
}
