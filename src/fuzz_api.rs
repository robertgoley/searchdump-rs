//! Entry points for the cargo-fuzz targets in `fuzz/` (TST-API-084). Built
//! only with `--features fuzz` (and in unit tests, which smoke-run each one).
//! Every function takes arbitrary bytes and must never panic.

use std::cell::RefCell;

use crate::pipeline::json_split::{ValueSplitter, single_line};

/// NDJSON/concatenated splitter: the first byte picks the chunk size.
pub fn json_split(data: &[u8]) {
    let Some((&n, rest)) = data.split_first() else {
        return;
    };
    let chunk = (n as usize % 64) + 1;
    let mut s = ValueSplitter::with_max_value(Some(1 << 20));
    for c in rest.chunks(chunk) {
        s.push(c);
        while let Ok(Some(v)) = s.next_value(false) {
            let line = single_line(v);
            assert!(!line.contains(&b'\n'));
        }
    }
    while let Ok(Some(v)) = s.next_value(true) {
        let _ = single_line(v);
    }
}

/// Compressed input as `file_source` reads it (TST-SUP-020): the bytes go through
/// `decode_input` (magic-byte detection, zstd window cap) and the JSON splitter with a
/// 1 MiB document cap. Output is bounded at 64 MiB per input so bombs end quickly.
fn compressed_input(magic: &[u8], data: &[u8]) {
    use std::io::Read;
    let mut bytes = data.to_vec();
    if !bytes.starts_with(magic) {
        bytes.splice(0..0, magic.iter().copied());
    }
    let Ok(mut r) =
        crate::pipeline::file_source::decode_input(Box::new(std::io::Cursor::new(bytes)), None)
    else {
        return;
    };
    let mut s = ValueSplitter::with_max_value(Some(1 << 20));
    let mut buf = vec![0u8; 64 << 10];
    let mut total = 0usize;
    while total < 64 << 20 {
        match r.read(&mut buf) {
            Ok(0) => {
                while let Ok(Some(v)) = s.next_value(true) {
                    let _ = single_line(v);
                }
                return;
            }
            Ok(n) => {
                total += n;
                s.push(&buf[..n]);
                loop {
                    match s.next_value(false) {
                        Ok(Some(v)) => {
                            let _ = single_line(v);
                        }
                        Ok(None) => break,
                        Err(_) => return,
                    }
                }
            }
            Err(_) => return,
        }
    }
}

/// `.zst` input: frames (concatenated, skippable, truncated, huge windows).
pub fn zstd_in(data: &[u8]) {
    compressed_input(&crate::pipeline::file_source::ZSTD_MAGIC, data);
}

/// `.gz` input: members (multi-member, trailing garbage, truncated).
pub fn gzip_in(data: &[u8]) {
    compressed_input(&[0x1f, 0x8b], data);
}

/// sonic-rs against serde_json (TST-SUP-022), on the calls this crate relies on:
/// 1. `check_doc_line` (sonic-rs validation + escape check) accepts exactly the lines
///    serde_json parses as one object (lone `\uD800`-style surrogate escapes excepted:
///    serde_json refuses them, JSON and the engines accept them);
/// 2. for valid objects, `to_object_iter` yields every member, each raw span parses
///    on its own to serde_json's value (last duplicate wins in both), and `get` finds
///    the first member's value;
/// 3. for valid arrays, `to_array_iter` spans match element by element.
pub fn sonic_vs_serde(data: &[u8]) {
    use serde_json::Value;
    let serde = serde_json::from_slice::<Value>(data);
    // any surrogate escape: serde_json refuses unpaired ones (with varying messages)
    let surrogate_escape = data.windows(4).any(|w| {
        w[0] == b'\\'
            && w[1] == b'u'
            && matches!(w[2], b'd' | b'D')
            && matches!(w[3], b'8'..=b'9' | b'a'..=b'f' | b'A'..=b'F')
    });
    let first = data.iter().find(|b| !b.is_ascii_whitespace());
    let serde_line_ok = match &serde {
        Ok(v) => v.is_object() && !data.iter().any(|b| *b == b'\n' || *b == b'\r'),
        Err(_) => false,
    };
    let ours = crate::pipeline::bulk::check_doc_line(data).is_ok();
    // serde_json's arbitrary_precision reserves this key (its own quirk, not sonic-rs)
    let reserved = memchr::memmem::find(data, b"$serde_json::private").is_some();
    if reserved {
        return;
    }
    let skip = serde.is_err() && surrogate_escape;
    if !skip && first == Some(&b'{') {
        assert_eq!(
            ours, serde_line_ok,
            "check_doc_line vs serde_json: {serde:?}"
        );
    }
    let span = |raw: &str| serde_json::from_str::<Value>(raw).expect("span is one JSON value");
    match serde {
        Ok(Value::Object(map)) => {
            let mut merged = serde_json::Map::new();
            let mut first_member = None;
            for m in sonic_rs::to_object_iter(data) {
                let (k, v) = m.expect("sonic-rs rejected a member of a valid object");
                let val = span(v.as_raw_str());
                first_member.get_or_insert_with(|| (k.to_string(), val.clone()));
                merged.insert(k.to_string(), val);
            }
            assert_eq!(Value::Object(merged), Value::Object(map.clone()));
            if let Some((k, v)) = first_member {
                let got = sonic_rs::get(data, [k.as_str()]).expect("get of an existing key");
                assert_eq!(span(got.as_raw_str()), v, "get({k:?})");
            }
        }
        Ok(Value::Array(items)) => {
            let got: Vec<Value> = sonic_rs::to_array_iter(data)
                .map(|e| span(e.expect("sonic-rs rejected an element").as_raw_str()))
                .collect();
            assert_eq!(got, items);
        }
        _ => {}
    }
}

/// Metadata line decoding and re-encoding.
pub fn codec_decode(data: &[u8]) {
    use crate::meta::codec::{LineEncoding, decode_any, encode_line};
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(data) else {
        return;
    };
    if let Ok(doc) = decode_any(v) {
        for enc in [LineEncoding::StringLiteral, LineEncoding::Object] {
            let line = encode_line(&doc, enc).expect("encode");
            let back: serde_json::Value = serde_json::from_str(&line).expect("re-parse");
            assert_eq!(decode_any(back).expect("decode"), doc);
        }
    }
}

thread_local! {
    static SINK: RefCell<crate::pipeline::BulkSink> = RefCell::new(fuzz_sink());
}

fn fuzz_sink() -> crate::pipeline::BulkSink {
    use crate::http::{
        ClientOptions, ClusterInfo, EsClient, Flavor, RetryPolicy, gate::RequestGate,
    };
    let client = EsClient::new(
        &ClientOptions::new(url::Url::parse("http://localhost:9200/").unwrap()),
        RequestGate::default(),
    )
    .expect("client");
    let opts = crate::pipeline::BulkOptions {
        info: ClusterInfo::new(Flavor::OpenSearch, "2.19.0"),
        index: None,
        doc_type: None,
        prefix: String::new(),
        suffix: String::new(),
        rename: None,
        action: crate::pipeline::BulkAction::Index,
        handle_version: true,
        version_type: None,
        meta_fields: vec!["routing".into(), "parent".into()],
        refresh: crate::pipeline::RefreshPolicy::Off,
        ignore_es_write_errors: true,
        ignore_errors: false,
        max_bulk_bytes: 1 << 20,
        retry: RetryPolicy::none(),
    };
    crate::pipeline::BulkSink::new(client, opts, tokio_util::sync::CancellationToken::new())
}

/// Bulk action encoding of one input line: the output is always exactly
/// one action line (plus one source line), each valid JSON (INJ-F1).
/// Mirrors the write path: `encode_doc` only ever sees lines that passed
/// `check_doc_line` (a trailing fragment after the object is rejected there).
pub fn bulk_encode_doc(data: &[u8]) {
    let line = single_line(data.to_vec());
    if crate::pipeline::bulk::check_doc_line(&line).is_err() {
        return;
    }
    let mut out = Vec::new();
    let ok = SINK.with(|s| s.borrow_mut().encode_doc(&line, &mut out).is_ok());
    if ok {
        let lines: Vec<&[u8]> = out
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .collect();
        assert!((1..=2).contains(&lines.len()), "{} lines", lines.len());
        for l in lines {
            serde_json::from_slice::<serde_json::Value>(l).expect("encoded line is JSON");
        }
    }
}

/// Search response extraction (scroll and PIT shapes).
pub fn extract_batch(data: &[u8]) {
    crate::retrieval::fuzz_extract_batch(data);
}

/// Bulk response accounting.
pub fn bulk_handle_response(data: &[u8]) {
    let docs: Vec<&[u8]> = vec![
        br#"{"_id":"1","_source":{}}"#,
        br#"{"_id":"2","_source":{}}"#,
    ];
    SINK.with(|s| {
        let _ = s.borrow_mut().fuzz_handle_response(data, &docs);
    });
}

/// `--fileSize` values.
pub fn parse_size(data: &[u8]) {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = crate::pipeline::file_sink::parse_size(s);
    }
}

/// Endpoint parsing and redaction.
pub fn endpoint_parse(data: &[u8]) {
    if let Ok(s) = std::str::from_utf8(data) {
        for dir in [
            crate::endpoint::Direction::Input,
            crate::endpoint::Direction::Output,
        ] {
            let _ = crate::endpoint::parse(s, dir, None);
        }
        let _ = crate::endpoint::redacted_endpoint(s);
    }
}

/// ILM → ISM conversion of an arbitrary policy body.
pub fn ilm_to_ism_convert(data: &[u8]) {
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(data) {
        for lenient in [false, true] {
            let _ = crate::meta::policy::ilm_to_ism::convert("p", &v, lenient);
        }
    }
}

/// Rename rules: `spec\nname\nname...`.
pub fn renamer_new_index(data: &[u8]) {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let mut lines = s.lines();
    let Some(spec) = lines.next() else { return };
    let rules = [spec.to_string()];
    for (p, x) in [(&rules[..], &[][..]), (&[][..], &rules[..])] {
        if let Ok(r) = crate::rename::IndexRenamer::new(p, x, "", "") {
            for n in lines.clone().take(8) {
                // INJ-015 fuzz_rename_output: an accepted output is a valid index name
                if let Ok(out) = r.index(n) {
                    assert!(
                        crate::rename::validate_index_name(&out).is_ok(),
                        "rename {n:?} -> invalid {out:?}"
                    );
                }
                let _ = r.alias(n);
                let _ = r.pattern(n);
            }
        }
    }
}

/// INJ-015 `fuzz_file_to_bulk`: bytes -> splitter -> `single_line` -> line check ->
/// `encode_doc`, as the restore path does. Oracle: exactly 2 NDJSON lines per accepted
/// document (index action), each line one valid JSON value, and every `_index` a valid
/// index name.
pub fn file_to_bulk(data: &[u8]) {
    let mut s = ValueSplitter::with_max_value(Some(1 << 20));
    s.push(data);
    let mut accepted = 0usize;
    let mut out = Vec::new();
    while let Ok(Some(v)) = s.next_value(true) {
        let line = single_line(v);
        if crate::pipeline::bulk::check_doc_line(&line).is_err() {
            continue;
        }
        let before = out.len();
        if SINK.with(|k| k.borrow_mut().encode_doc(&line, &mut out).is_ok()) {
            accepted += 1;
        } else {
            out.truncate(before);
        }
    }
    let lines: Vec<&[u8]> = out
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(lines.len(), 2 * accepted);
    for pair in lines.chunks(2) {
        assert!(crate::pipeline::bulk::check_doc_line(pair[0]).is_ok());
        assert!(crate::pipeline::bulk::check_doc_line(pair[1]).is_ok());
        let action: serde_json::Value = serde_json::from_slice(pair[0]).expect("action line");
        let idx = action["index"]["_index"].as_str().expect("_index");
        assert!(crate::rename::validate_index_name(idx).is_ok(), "{idx:?}");
    }
}

/// INJ-015 `fuzz_url_segments`: arbitrary path segments (split at NUL) never produce a
/// URL whose path holds an empty, `.` or `..` segment, and every segment round-trips.
pub fn url_segments(data: &[u8]) {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let segs: Vec<&str> = s.split('\0').take(6).collect();
    SINK.with(|k| {
        let k = k.borrow();
        if let Ok(url) = k.fuzz_client().url(&segs, &[("q", s)], false) {
            let got: Vec<String> = url
                .path_segments()
                .unwrap()
                .map(|p| {
                    percent_encoding::percent_decode_str(p)
                        .decode_utf8_lossy()
                        .into_owned()
                })
                .collect();
            assert_eq!(got, segs, "{url}");
            assert!(got.iter().all(|g| !g.is_empty() && g != "." && g != ".."));
        }
    });
}

/// INJ-015 `fuzz_meta_names`: whatever `validate_index_name` accepts meets the
/// Elasticsearch/OpenSearch rules written out independently here.
pub fn meta_names(data: &[u8]) {
    let Ok(name) = std::str::from_utf8(data) else {
        return;
    };
    if crate::rename::validate_index_name(name).is_ok() {
        assert!(!name.is_empty() && name.len() <= 255 && name != "." && name != "..");
        assert!(!name.starts_with(['-', '_', '+']));
        assert!(!name.chars().any(|c| "\\/*?\"<>| ,#:".contains(c)));
        assert_eq!(name.to_lowercase(), name);
    }
}

/// `--httpAuthFile` contents.
pub fn ini_auth_file(data: &[u8]) {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = crate::endpoint::parse_auth_ini(s);
    }
}

/// Secret scrubbing (L-645 / D-4): `user:password`; once the password and
/// its Basic token are registered, no rendering of the password (raw, URL
/// userinfo, percent/form-encoded, JSON, `{:?}`, Basic token) survives.
pub fn secret_redaction(data: &[u8]) {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let (user, pw) = s.split_once(':').unwrap_or(("u", s));
    let pw = pw.trim();
    if pw.len() < crate::secrets::MIN_LEN {
        return;
    }
    let mut r = crate::secrets::Registry::new();
    r.add(pw);
    r.add(&base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        format!("{user}:{pw}"),
    ));
    for (msg, needle) in crate::secrets::renderings(user, pw) {
        let out = r.scrub(&msg);
        assert!(!out.contains(&needle), "{needle:?} survived in {out:?}");
    }
}

#[cfg(test)]
mod tests {
    /// Each entry point survives a few awkward inputs (the fuzz smoke runs
    /// cover the rest).
    #[test]
    fn entry_points_never_panic_on_samples() {
        let samples: &[&[u8]] = &[
            b"",
            b"\x00",
            b"{}",
            b"\x05{\"a\":1}\n{\"b\":[1,2]}",
            br#""{\"a\":1}""#,
            br#"{"_index":"i","_id":"1","_source":{"a":1},"_routing":"r","_version":3}"#,
            br#"{"hits":{"total":1,"hits":[{"_id":"1"}]}}"#,
            br#"{"errors":true,"items":[{"index":{"status":429}},{"index":{"status":201}}]}"#,
            b"10mb",
            b"http://u:p@h:9200/i",
            br#"{"policy":{"phases":{"hot":{"actions":{"rollover":{"max_age":"1d"}}}}}}"#,
            b"^(a|b)-(.*):x-$2\na-1\nb-1",
            b"user=u\npassword=p\n",
            "fake:FAKE-pw: @%#/?\u{e9}\\\"x***".as_bytes(),
            b"\xff\xfe",
            // L-507 regression: lenient conversion of a policy without phases panicked
            br#"{"phases":{}}"#,
            // L-509: lone surrogate escape (serde_json refuses, JSON allows)
            br#"{"s":"\ud800"}"#,
            // L-512: a search response with an invalid escape inside _source
            br#"{"hits":{"hits":[{"_index":"i","_id":"1","_source":{"k":"\u27-"}}]}}"#,
            // L-505 regression: ASan heap-buffer-overflow in sonic-rs string serialisation
            // (fixed by its `sanitize` feature; only visible in an ASan release build).
            br#"{"_index":"golden_dst","_id":"r4","_source":{"k":"d","n":4}}"#,
            // a trailing fragment after the object: rejected before encoding
            br#"{"_index":"golden_src","_type":"_scoreGdoc 9"}}"#,
            // an invalid \u escape inside a string value: rejected before encoding
            br#"{"_index":"golden_dst","_source":{"k":"a","n":"%%_i\u%%%%"}}"#,
        ];
        for s in samples {
            super::json_split(s);
            super::codec_decode(s);
            super::bulk_encode_doc(s);
            super::extract_batch(s);
            super::bulk_handle_response(s);
            super::parse_size(s);
            super::endpoint_parse(s);
            super::ilm_to_ism_convert(s);
            super::renamer_new_index(s);
            super::ini_auth_file(s);
            super::zstd_in(s);
            super::gzip_in(s);
            super::sonic_vs_serde(s);
            super::file_to_bulk(s);
            super::url_segments(s);
            super::meta_names(s);
            super::secret_redaction(s);
        }
    }
}
