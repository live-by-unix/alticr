//! OSC (Operating System Command) sequence scanner.
//!
//! Extracts `ESC ] Ps ; Pt BEL` / `ESC ] Ps ; Pt ESC \` sequences from a
//! byte stream. OSC sequences may be split across arbitrary PTY reads, so
//! the scanner keeps a small carry-over buffer between calls.

/// Maximum length of a single OSC payload before we give up on it.
const MAX_OSC_LEN: usize = 1024 * 1024;

/// One completed OSC sequence: `(ps, pt)` where `pt` is the raw text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OscSequence {
    pub ps: u16,
    pub pt: String,
}

/// Stateful scanner for OSC sequences embedded in a byte stream.
///
/// Feed it every chunk of PTY output; it returns any complete OSC
/// sequences found. Other bytes (including non-OSC escapes) are ignored.
#[derive(Debug, Default)]
pub struct OscScanner {
    /// Pending bytes that may be part of an unfinished OSC sequence.
    pending: Vec<u8>,
    /// We are currently inside an OSC payload.
    in_osc: bool,
}

impl OscScanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a chunk of output. Returns all complete OSC sequences it
    /// contains (in order).
    pub fn feed(&mut self, data: &[u8]) -> Vec<OscSequence> {
        let mut out = Vec::new();
        let mut i = 0;

        if self.in_osc {
            // Continue a sequence started earlier.
            match self.consume_until_terminator(data, &mut out) {
                ConsumeResult::Done(consumed) => i = consumed,
                ConsumeResult::NeedMore => return out,
                ConsumeResult::Overflow => {
                    self.in_osc = false;
                    self.pending.clear();
                    return out;
                }
            }
        }

        while i < data.len() {
            // Look for ESC ] to start an OSC.
            if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b']' {
                // Collect the payload bytes until BEL or ESC \.
                let mut j = i + 2;
                let mut payload_end = None;
                while j < data.len() {
                    if data[j] == 0x07 {
                        payload_end = Some((j, 1));
                        break;
                    }
                    if data[j] == 0x1b && j + 1 < data.len() && data[j + 1] == b'\\' {
                        payload_end = Some((j, 2));
                        break;
                    }
                    // A bare ESC not followed by '\' is malformed; abort.
                    if data[j] == 0x1b {
                        break;
                    }
                    j += 1;
                }

                match payload_end {
                    Some((end, term_len)) => {
                        let payload = &data[i + 2..end];
                        if let Some(seq) = parse_payload(payload) {
                            out.push(seq);
                        }
                        i = end + term_len;
                    }
                    None => {
                        // Terminator may be split across reads, or the
                        // sequence may extend beyond this chunk.
                        let rest = &data[i + 2..];
                        if rest.len() > MAX_OSC_LEN {
                            // Runaway sequence; drop it.
                            return out;
                        }
                        self.pending.clear();
                        self.pending.extend_from_slice(rest);
                        self.in_osc = true;
                        return out;
                    }
                }
            } else {
                i += 1;
            }
        }

        out
    }

    /// Continue consuming a pending OSC payload. Returns how many bytes of
    /// `data` were consumed, or signals that more data is needed.
    fn consume_until_terminator(
        &mut self,
        data: &[u8],
        out: &mut Vec<OscSequence>,
    ) -> ConsumeResult {
        // First check whether pending already contains a terminator
        // (it may have been split exactly at the boundary).
        if let Some((pos, term_len)) = find_terminator(&self.pending) {
            let mut payload = std::mem::take(&mut self.pending);
            payload.truncate(pos);
            if let Some(seq) = parse_payload(&payload) {
                out.push(seq);
            }
            self.in_osc = false;
            // Bytes after the terminator in `pending` are impossible:
            // pending only ever holds payload bytes. Re-feed remaining
            // data normally by consuming nothing extra.
            let _ = term_len;
            return ConsumeResult::Done(0);
        }

        // Terminator may straddle the boundary: pending ends with ESC and
        // the new chunk starts with '\'.
        if self.pending.last() == Some(&0x1b) && data.first() == Some(&b'\\') {
            let mut payload = std::mem::take(&mut self.pending);
            payload.pop(); // drop the dangling ESC
            if payload.len() > MAX_OSC_LEN {
                self.in_osc = false;
                return ConsumeResult::Overflow;
            }
            if let Some(seq) = parse_payload(&payload) {
                out.push(seq);
            }
            self.in_osc = false;
            return ConsumeResult::Done(1);
        }

        // Scan new data for the terminator.
        if let Some((pos, term_len)) = find_terminator(data) {
            let mut payload = std::mem::take(&mut self.pending);
            payload.extend_from_slice(&data[..pos]);
            if payload.len() > MAX_OSC_LEN {
                self.in_osc = false;
                return ConsumeResult::Overflow;
            }
            if let Some(seq) = parse_payload(&payload) {
                out.push(seq);
            }
            self.in_osc = false;
            return ConsumeResult::Done(pos + term_len);
        }

        // No terminator yet; append what we have (bounded).
        if self.pending.len() + data.len() > MAX_OSC_LEN {
            self.in_osc = false;
            self.pending.clear();
            return ConsumeResult::Overflow;
        }
        self.pending.extend_from_slice(data);
        ConsumeResult::NeedMore
    }
}

enum ConsumeResult {
    /// Consumed `usize` bytes of the input chunk.
    Done(usize),
    NeedMore,
    Overflow,
}

/// Find BEL or ST terminator in `data`.
fn find_terminator(data: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0x07 {
            return Some((i, 1));
        }
        if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b'\\' {
            return Some((i, 2));
        }
        i += 1;
    }
    None
}

/// Parse `Ps ; Pt` payload into an OscSequence.
fn parse_payload(payload: &[u8]) -> Option<OscSequence> {
    let semi = payload.iter().position(|&b| b == b';')?;
    let ps_str = std::str::from_utf8(&payload[..semi]).ok()?;
    let ps: u16 = ps_str.trim().parse().ok()?;
    // OSC payloads are technically ISO-8859-1, but UTF-8 is the modern
    // convention; lossy conversion is fine for titles/cwd/clipboard.
    let pt = String::from_utf8_lossy(&payload[semi + 1..]).into_owned();
    Some(OscSequence { ps, pt })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_title() {
        let mut s = OscScanner::new();
        let out = s.feed(b"hello\x1b]0;my title\x07world");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].ps, 0);
        assert_eq!(out[0].pt, "my title");
    }

    #[test]
    fn st_terminated() {
        let mut s = OscScanner::new();
        let out = s.feed(b"\x1b]2;win\x1b\\rest");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].ps, 2);
        assert_eq!(out[0].pt, "win");
    }

    #[test]
    fn split_across_reads() {
        let mut s = OscScanner::new();
        assert!(s.feed(b"abc\x1b]0;par").is_empty());
        assert!(s.feed(b"tial tit").is_empty());
        let out = s.feed(b"le\x07done");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].pt, "partial title");
    }

    #[test]
    fn split_terminator() {
        let mut s = OscScanner::new();
        s.feed(b"\x1b]7;file://host/some/path\x1b");
        let out = s.feed(b"\\next");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].ps, 7);
    }

    #[test]
    fn multiple_in_chunk() {
        let mut s = OscScanner::new();
        let out = s.feed(b"\x1b]0;a\x07x\x1b]2;b\x07y");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].pt, "a");
        assert_eq!(out[1].pt, "b");
    }

    #[test]
    fn non_osc_escape_ignored() {
        let mut s = OscScanner::new();
        let out = s.feed(b"\x1b[31mred\x1b[0m \x1b]notreally");
        assert!(out.is_empty());
    }
}
