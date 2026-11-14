//! Incremental splitter for a stream of top-level JSON values: NDJSON,
//! concatenated JSON and pretty-printed JSON all work. Values are returned as
//! raw bytes (lossless) and minified onto a single line when needed.

/// Feed bytes with [`ValueSplitter::push`]; complete values come out of
/// [`ValueSplitter::next_value`].
#[derive(Debug, Default)]
pub struct ValueSplitter {
    buf: Vec<u8>,
    /// Scan position inside `buf`.
    pos: usize,
    /// Start of the value being scanned, if one has begun.
    start: Option<usize>,
    depth: u32,
    in_string: bool,
    escaped: bool,
    /// Scalar top-level value (number/literal/string) being scanned.
    scalar: bool,
    /// Largest value accepted, in bytes (`None` = unlimited). Buffering
    /// stops as soon as a value grows past it, so a giant value cannot
    /// exhaust memory.
    max_value: Option<usize>,
    /// Test only: scan byte by byte (the pre-L-808q path) to compare against.
    #[cfg(test)]
    no_skip: bool,
}

/// Bytes that change the splitter's state inside an object or array, outside strings.
const STRUCTURAL: [bool; 256] = {
    let mut t = [false; 256];
    t[b'"' as usize] = true;
    t[b'{' as usize] = true;
    t[b'[' as usize] = true;
    t[b'}' as usize] = true;
    t[b']' as usize] = true;
    t
};

/// Capacity a reused byte buffer may keep whatever it holds (L-802a).
pub(crate) const RETAIN_FLOOR: usize = 4 << 20;

/// Gives back the capacity a reused buffer kept after an outlier (L-802a). One large
/// document (up to `--maxDocBytes`, hundreds of MB) grew the buffer to its size, and
/// `clear`/`drain` keep the capacity, so it stayed resident for the rest of the run.
/// Keeps `max(RETAIN_FLOOR, 2 x len)` and only shrinks above twice that, so a buffer
/// in its normal size range is never reallocated.
pub(crate) fn release_excess(buf: &mut Vec<u8>) {
    let keep = RETAIN_FLOOR.max(buf.len().saturating_mul(2));
    if buf.capacity() > keep.saturating_mul(2) {
        buf.shrink_to(keep);
    }
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\n' | b'\r' | b'\t') || b == 0xEF || b == 0xBB || b == 0xBF
}

impl ValueSplitter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Refuse values larger than `max` bytes (see [`ValueSplitter::next_value`]).
    pub fn with_max_value(max: Option<usize>) -> Self {
        Self {
            max_value: max,
            ..Self::default()
        }
    }

    fn check_size(&self, len: usize) -> Result<(), String> {
        match self.max_value {
            Some(max) if len > max => Err(format!(
                "a single document is larger than {} (it was not read further)",
                human_size(max)
            )),
            _ => Ok(()),
        }
    }

    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    fn compact(&mut self) {
        let keep_from = self.start.unwrap_or(self.pos);
        if keep_from > 0 {
            self.buf.drain(..keep_from);
            self.pos -= keep_from;
            if let Some(s) = self.start.as_mut() {
                *s = 0;
            }
        }
        release_excess(&mut self.buf);
    }

    /// Next complete value, if any. With `eof`, a trailing scalar without a
    /// terminator is returned too, and an unterminated value is an error.
    pub fn next_value(&mut self, eof: bool) -> Result<Option<Vec<u8>>, String> {
        loop {
            if self.pos >= self.buf.len() {
                if eof && let Some(start) = self.start {
                    if self.scalar && !self.in_string {
                        self.check_size(self.buf.len() - start)?;
                        let v = self.buf[start..].to_vec();
                        self.reset_value();
                        self.buf.clear();
                        release_excess(&mut self.buf);
                        self.pos = 0;
                        return Ok(Some(v));
                    }
                    return Err("unexpected end of input inside a JSON value".to_string());
                }
                if let Some(start) = self.start {
                    self.check_size(self.buf.len() - start)?;
                }
                self.compact();
                return Ok(None);
            }
            if self.start.is_some() && !self.scalar && self.fast() {
                // L-808q: jump over bytes that cannot change the state (SIMD inside strings,
                // a table lookup outside them); the byte-by-byte code below then handles the
                // byte found, so values, errors and buffering are exactly as before.
                let rest = &self.buf[self.pos..];
                let skip = if self.in_string {
                    if self.escaped {
                        Some(0)
                    } else {
                        memchr::memchr2(b'"', b'\\', rest)
                    }
                } else {
                    rest.iter().position(|&b| STRUCTURAL[b as usize])
                };
                match skip {
                    Some(k) => self.pos += k,
                    None => {
                        self.pos = self.buf.len();
                        continue;
                    }
                }
            }
            let b = self.buf[self.pos];
            match self.start {
                None => {
                    if is_ws(b) || b == b',' {
                        self.pos += 1;
                        continue;
                    }
                    self.start = Some(self.pos);
                    match b {
                        b'{' | b'[' => {
                            self.depth = 1;
                            self.scalar = false;
                        }
                        b'"' => {
                            self.scalar = true;
                            self.in_string = true;
                        }
                        _ => self.scalar = true,
                    }
                    self.pos += 1;
                }
                Some(start) => {
                    if self.in_string {
                        if self.escaped {
                            self.escaped = false;
                        } else if b == b'\\' {
                            self.escaped = true;
                        } else if b == b'"' {
                            self.in_string = false;
                        }
                        self.pos += 1;
                        continue;
                    }
                    if self.scalar {
                        if is_ws(b) || b == b',' || b == b'{' || b == b'[' {
                            self.check_size(self.pos - start)?;
                            let v = self.buf[start..self.pos].to_vec();
                            self.reset_value();
                            return Ok(Some(v));
                        }
                        if b == b'"' {
                            self.in_string = true;
                        }
                        self.pos += 1;
                        continue;
                    }
                    match b {
                        b'"' => self.in_string = true,
                        b'{' | b'[' => self.depth += 1,
                        b'}' | b']' => {
                            self.depth -= 1;
                            if self.depth == 0 {
                                self.pos += 1;
                                self.check_size(self.pos - start)?;
                                let v = self.buf[start..self.pos].to_vec();
                                self.reset_value();
                                return Ok(Some(v));
                            }
                        }
                        _ => {}
                    }
                    self.pos += 1;
                }
            }
        }
    }

    #[cfg(not(test))]
    fn fast(&self) -> bool {
        true
    }

    #[cfg(test)]
    fn fast(&self) -> bool {
        !self.no_skip
    }

    fn reset_value(&mut self) {
        self.start = None;
        self.depth = 0;
        self.in_string = false;
        self.escaped = false;
        self.scalar = false;
    }
}

fn human_size(n: usize) -> String {
    if n >= 1 << 20 && n.is_multiple_of(1 << 20) {
        format!("{} MB", n >> 20)
    } else {
        format!("{n} bytes")
    }
}

/// Remove insignificant whitespace (outside strings) so a value fits on one
/// line. Returns the input unchanged when it has no line breaks.
///
/// Raw CR/LF bytes *inside* strings (invalid JSON, but a lenient parser may
/// let them through) are escaped as `\r`/`\n`, so the result never contains
/// a line break: one record is always exactly one NDJSON line (INJ-F1).
pub fn single_line(value: Vec<u8>) -> Vec<u8> {
    if memchr::memchr2(b'\n', b'\r', &value).is_none() {
        return value;
    }
    flatten(&value)
}

/// Borrowing variant of [`single_line`]: no copy in the common case.
pub fn single_line_slice(value: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    if memchr::memchr2(b'\n', b'\r', value).is_none() {
        return std::borrow::Cow::Borrowed(value);
    }
    std::borrow::Cow::Owned(flatten(value))
}

fn flatten(value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    let mut in_string = false;
    let mut escaped = false;
    for &b in value {
        if in_string {
            match b {
                b'\n' => out.extend_from_slice(b"\\n"),
                b'\r' => out.extend_from_slice(b"\\r"),
                _ => out.push(b),
            }
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b' ' | b'\n' | b'\r' | b'\t' => {}
            b'"' => {
                in_string = true;
                out.push(b);
            }
            _ => out.push(b),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(input: &[u8], chunk: usize) -> Vec<String> {
        let mut s = ValueSplitter::new();
        let mut out = vec![];
        for c in input.chunks(chunk.max(1)) {
            s.push(c);
            while let Some(v) = s.next_value(false).unwrap() {
                out.push(String::from_utf8(single_line(v)).unwrap());
            }
        }
        while let Some(v) = s.next_value(true).unwrap() {
            out.push(String::from_utf8(single_line(v)).unwrap());
        }
        out
    }

    #[test]
    fn ndjson_concatenated_and_pretty() {
        let input = b"{\"a\":1}\n{\"b\":\"x}\\\"{\"}{\"c\":[1,{\"d\":2}]}\n{\n  \"e\": \"keep  spaces\",\n  \"f\": 1\n}\n";
        for chunk in [1, 3, 7, 1000] {
            assert_eq!(
                all(input, chunk),
                vec![
                    r#"{"a":1}"#,
                    r#"{"b":"x}\"{"}"#,
                    r#"{"c":[1,{"d":2}]}"#,
                    r#"{"e":"keep  spaces","f":1}"#
                ],
                "chunk {chunk}"
            );
        }
    }

    #[test]
    fn crlf_and_bom_and_big_ints() {
        let input = b"\xEF\xBB\xBF{\"n\":123456789012345678901234567890}\r\n{\"m\":-0.5e10}\r\n";
        assert_eq!(
            all(input, 4),
            vec![
                r#"{"n":123456789012345678901234567890}"#,
                r#"{"m":-0.5e10}"#
            ]
        );
    }

    #[test]
    fn top_level_scalars_and_strings() {
        assert_eq!(
            all(b"\"{\\\"x\\\":1}\"\n42", 2),
            vec![r#""{\"x\":1}""#, "42"]
        );
    }

    #[test]
    fn values_over_the_size_cap_are_refused_without_buffering_them() {
        // a complete value just over the cap, inside one chunk
        let mut s = ValueSplitter::with_max_value(Some(16));
        s.push(b"{\"a\":1}\n{\"b\":\"0123456789abcdef\"}\n");
        assert_eq!(s.next_value(false).unwrap().unwrap(), b"{\"a\":1}");
        let e = s.next_value(false).unwrap_err();
        assert!(e.contains("larger than 16 bytes"), "{e}");
        // an unterminated giant value is refused once the buffer passes the cap
        let mut s = ValueSplitter::with_max_value(Some(1 << 20));
        s.push(b"{\"big\":\"");
        let chunk = vec![b'x'; 64 << 10];
        let mut refused = None;
        for i in 0..64 {
            s.push(&chunk);
            if let Err(e) = s.next_value(false) {
                refused = Some((i, e));
                break;
            }
        }
        let (i, e) = refused.expect("refused");
        assert!(i <= 16, "refused after {i} chunks");
        assert!(e.contains("larger than 1 MB"), "{e}");
        // a scalar at EOF and at the cap exactly is fine
        let mut s = ValueSplitter::with_max_value(Some(4));
        s.push(b"1234");
        assert_eq!(s.next_value(true).unwrap().unwrap(), b"1234");
    }

    #[test]
    fn buffer_capacity_is_given_back_after_a_large_document() {
        // L-802a: one 32 MiB document must not keep 32 MiB+ resident for the rest of the run.
        let mut s = ValueSplitter::new();
        let mut big = b"{\"big\":\"".to_vec();
        big.resize(big.len() + (32 << 20), b'x');
        big.extend_from_slice(b"\"}\n");
        let chunks: Vec<&[u8]> = big.chunks(256 << 10).collect();
        let (last, head) = chunks.split_last().unwrap();
        for c in head {
            s.push(c);
            assert!(s.next_value(false).unwrap().is_none());
        }
        assert!(
            s.buf.capacity() >= 31 << 20,
            "the large value is buffered whole"
        );
        s.push(last);
        assert_eq!(s.next_value(false).unwrap().unwrap().len(), big.len() - 1);
        assert!(s.next_value(false).unwrap().is_none());
        let small = b"{\"a\":1}\n".repeat(1000);
        for _ in 0..4 {
            s.push(&small);
            while let Some(v) = s.next_value(false).unwrap() {
                assert_eq!(v, b"{\"a\":1}");
            }
        }
        assert!(
            s.buf.capacity() <= 2 * RETAIN_FLOOR,
            "capacity {} kept after the large document",
            s.buf.capacity()
        );
    }

    #[test]
    fn release_excess_leaves_normal_buffers_alone() {
        // A buffer in its usual range keeps its capacity (no realloc per batch) ...
        let mut b = Vec::with_capacity(RETAIN_FLOOR);
        b.extend_from_slice(&[1u8; 1000]);
        release_excess(&mut b);
        assert_eq!(b.capacity(), RETAIN_FLOOR);
        // ... and so does one whose contents fill most of it.
        let mut b = vec![0u8; 20 << 20];
        let cap = b.capacity();
        release_excess(&mut b);
        assert_eq!(b.capacity(), cap);
        assert_eq!(b.len(), 20 << 20);
        // An emptied outlier shrinks to the floor and keeps its contents.
        let mut b = vec![7u8; 64 << 20];
        b.truncate(10);
        release_excess(&mut b);
        assert!(b.capacity() <= RETAIN_FLOOR + 4096, "{}", b.capacity());
        assert_eq!(b, vec![7u8; 10]);
    }

    #[test]
    fn unterminated_value_is_an_error() {
        let mut s = ValueSplitter::new();
        s.push(b"{\"a\":");
        assert!(s.next_value(false).unwrap().is_none());
        assert!(s.next_value(true).is_err());
    }

    #[test]
    fn single_line_never_leaves_a_line_break() {
        let pretty = b"{\"a\":\r\n\t{\"_index\":\"victim\",\n \"s\":\"x\ny\r\"}\n}".to_vec();
        let flat = single_line(pretty.clone());
        assert!(memchr::memchr2(b'\n', b'\r', &flat).is_none());
        assert_eq!(flat, br#"{"a":{"_index":"victim","s":"x\ny\r"}}"#.to_vec());
        assert_eq!(single_line_slice(&pretty).into_owned(), flat);
        assert!(matches!(
            single_line_slice(b"{\"a\": 1}"),
            std::borrow::Cow::Borrowed(_)
        ));
    }
}

/// L-501 (TST-API-080): properties of the splitter. T0 runs 10k cases per
/// property (`PROPTEST_CASES` overrides; B runs 1M nightly).
#[cfg(test)]
mod props {
    use super::*;
    use proptest::prelude::*;
    use serde_json::Value;

    fn cases() -> u32 {
        std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10_000)
    }

    /// Strings full of the bytes that matter to the scanner.
    fn tricky_string() -> impl Strategy<Value = String> {
        proptest::collection::vec(
            prop_oneof![
                Just('{'),
                Just('}'),
                Just('['),
                Just(']'),
                Just('"'),
                Just('\\'),
                Just(','),
                Just(':'),
                Just('\n'),
                Just('\r'),
                Just('\t'),
                Just(' '),
                Just('\u{feff}'),
                Just('é'),
                Just('😀'),
                proptest::char::range('a', 'z'),
            ],
            0..12,
        )
        .prop_map(|v| v.into_iter().collect())
    }

    fn json_value() -> impl Strategy<Value = Value> {
        let leaf = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(Value::from),
            any::<u64>().prop_map(Value::from),
            tricky_string().prop_map(Value::String),
        ];
        leaf.prop_recursive(4, 32, 6, |inner| {
            prop_oneof![
                proptest::collection::vec(inner.clone(), 0..5).prop_map(Value::Array),
                proptest::collection::vec((tricky_string(), inner), 0..5)
                    .prop_map(|kv| Value::Object(kv.into_iter().collect())),
            ]
        })
    }

    /// Top-level values the way dumps hold them: objects (documents).
    fn document() -> impl Strategy<Value = Value> {
        proptest::collection::vec((tricky_string(), json_value()), 0..5)
            .prop_map(|kv| Value::Object(kv.into_iter().collect()))
    }

    /// Every value, error and end state of one splitter fed `input` in `chunk`-byte pieces.
    fn trace(input: &[u8], chunk: usize, max: Option<usize>, no_skip: bool) -> Vec<String> {
        let mut s = ValueSplitter::with_max_value(max);
        s.no_skip = no_skip;
        let mut out = vec![];
        let pieces: Vec<&[u8]> = input.chunks(chunk.max(1)).collect();
        for (i, c) in pieces.iter().enumerate() {
            s.push(c);
            let eof = i + 1 == pieces.len();
            loop {
                match s.next_value(eof) {
                    Ok(Some(v)) => out.push(format!("v {v:?}")),
                    Ok(None) => break,
                    Err(e) => {
                        out.push(format!("e {e}"));
                        return out;
                    }
                }
            }
            out.push(format!("@ {} {:?}", s.pos, s.start));
        }
        out
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(cases()))]
        /// L-808q: the skipping scan returns exactly what the byte-by-byte scan returns, on
        /// arbitrary (also malformed) bytes, any chunking and any size limit.
        #[test]
        fn skipping_scan_equals_byte_scan(
            input in proptest::collection::vec(
                prop_oneof![
                    Just(b'{'), Just(b'}'), Just(b'['), Just(b']'), Just(b'"'), Just(b'\\'),
                    Just(b','), Just(b':'), Just(b'\n'), Just(b' '), Just(0xEFu8), Just(b'1'),
                    Just(b't'), Just(b'a'), any::<u8>(),
                ],
                0..200,
            ),
            chunk in 1usize..64,
            max in proptest::option::of(1usize..80),
        ) {
            prop_assert_eq!(trace(&input, chunk, max, false), trace(&input, chunk, max, true));
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum Layout {
        Ndjson,
        Crlf,
        Pretty,
        Concatenated,
        CommaSeparated,
    }

    fn render(docs: &[Value], layout: Layout, bom: bool) -> Vec<u8> {
        let mut out = Vec::new();
        if bom {
            out.extend_from_slice(b"\xEF\xBB\xBF");
        }
        for (i, d) in docs.iter().enumerate() {
            match layout {
                Layout::Ndjson => {
                    out.extend(serde_json::to_vec(d).unwrap());
                    out.push(b'\n');
                }
                Layout::Crlf => {
                    out.extend(serde_json::to_vec(d).unwrap());
                    out.extend_from_slice(b"\r\n");
                }
                Layout::Pretty => {
                    out.extend(serde_json::to_vec_pretty(d).unwrap());
                    out.push(b'\n');
                }
                Layout::Concatenated => out.extend(serde_json::to_vec(d).unwrap()),
                Layout::CommaSeparated => {
                    if i > 0 {
                        out.extend_from_slice(b",\n");
                    }
                    out.extend(serde_json::to_vec(d).unwrap());
                }
            }
        }
        out
    }

    fn split_all(input: &[u8], chunks: &[usize]) -> Result<Vec<Vec<u8>>, String> {
        let mut s = ValueSplitter::new();
        let mut out = Vec::new();
        let mut pos = 0;
        let mut ci = 0;
        while pos < input.len() {
            let n = chunks
                .get(ci % chunks.len().max(1))
                .copied()
                .unwrap_or(7)
                .max(1);
            ci += 1;
            let end = (pos + n).min(input.len());
            s.push(&input[pos..end]);
            pos = end;
            while let Some(v) = s.next_value(false)? {
                out.push(v);
            }
        }
        while let Some(v) = s.next_value(true)? {
            out.push(v);
        }
        Ok(out)
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: cases(), ..ProptestConfig::default() })]

        /// Every layout, any chunking: exactly the documents, in order, and
        /// each one flattens to a single line that parses to the same value.
        #[test]
        fn documents_come_back_whole_in_order(
            docs in proptest::collection::vec(document(), 0..6),
            layout in prop_oneof![
                Just(Layout::Ndjson), Just(Layout::Crlf), Just(Layout::Pretty),
                Just(Layout::Concatenated), Just(Layout::CommaSeparated)
            ],
            bom in any::<bool>(),
            chunks in proptest::collection::vec(1usize..64, 1..8),
        ) {
            let input = render(&docs, layout, bom);
            let got = split_all(&input, &chunks).map_err(TestCaseError::fail)?;
            prop_assert_eq!(got.len(), docs.len());
            for (raw, want) in got.into_iter().zip(&docs) {
                let line = single_line(raw);
                prop_assert!(!line.contains(&b'\n') && !line.contains(&b'\r'));
                let back: Value = serde_json::from_slice(&line).map_err(|e| TestCaseError::fail(e.to_string()))?;
                prop_assert_eq!(&back, want);
            }
        }

        /// Arbitrary bytes in arbitrary chunks never panic, and a value
        /// returned is never longer than the input.
        #[test]
        fn arbitrary_bytes_never_panic(
            input in proptest::collection::vec(any::<u8>(), 0..256),
            chunks in proptest::collection::vec(1usize..32, 1..6),
        ) {
            if let Ok(values) = split_all(&input, &chunks) {
                let total: usize = values.iter().map(Vec::len).sum();
                prop_assert!(total <= input.len());
            }
        }

        /// Chunking never changes the result.
        #[test]
        fn chunking_is_irrelevant(
            input in proptest::collection::vec(
                prop_oneof![Just(b'{'), Just(b'}'), Just(b'"'), Just(b'\\'), Just(b'\n'), Just(b'1'), Just(b' '), Just(b'[')],
                0..64),
            chunks in proptest::collection::vec(1usize..16, 1..6),
        ) {
            let whole = split_all(&input, &[input.len().max(1)]);
            let parts = split_all(&input, &chunks);
            prop_assert_eq!(whole.is_ok(), parts.is_ok());
            if let (Ok(a), Ok(b)) = (whole, parts) {
                prop_assert_eq!(a, b);
            }
        }
    }
}
