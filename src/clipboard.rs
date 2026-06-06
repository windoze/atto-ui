use std::io::{self, Write};

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Builds an OSC52 sequence that asks the terminal to put `text` on the system clipboard.
///
/// OSC52 is a terminal escape sequence, so unsupported terminals can safely ignore it. The
/// sequence targets the standard clipboard selection (`c`) and uses BEL as the terminator.
pub fn osc52_sequence(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", encode_base64(text.as_bytes()))
}

/// Writes an OSC52 clipboard update to the provided terminal output stream.
pub fn write_osc52(mut writer: impl Write, text: &str) -> io::Result<()> {
    writer.write_all(osc52_sequence(text).as_bytes())?;
    writer.flush()
}

/// Best-effort system clipboard copy through the process stdout terminal.
pub fn copy_to_system_clipboard(text: &str) -> io::Result<()> {
    write_osc52(io::stdout(), text)
}

fn encode_base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut chunks = bytes.chunks_exact(3);

    for chunk in &mut chunks {
        let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | chunk[2] as u32;
        out.push(BASE64[((n >> 18) & 0x3f) as usize] as char);
        out.push(BASE64[((n >> 12) & 0x3f) as usize] as char);
        out.push(BASE64[((n >> 6) & 0x3f) as usize] as char);
        out.push(BASE64[(n & 0x3f) as usize] as char);
    }

    match chunks.remainder() {
        [a] => {
            let n = (*a as u32) << 16;
            out.push(BASE64[((n >> 18) & 0x3f) as usize] as char);
            out.push(BASE64[((n >> 12) & 0x3f) as usize] as char);
            out.push('=');
            out.push('=');
        }
        [a, b] => {
            let n = ((*a as u32) << 16) | ((*b as u32) << 8);
            out.push(BASE64[((n >> 18) & 0x3f) as usize] as char);
            out.push(BASE64[((n >> 12) & 0x3f) as usize] as char);
            out.push(BASE64[((n >> 6) & 0x3f) as usize] as char);
            out.push('=');
        }
        [] => {}
        _ => unreachable!("chunks_exact remainder is at most two bytes"),
    }

    out
}

#[cfg(test)]
mod tests {
    use super::{osc52_sequence, write_osc52};

    #[test]
    fn osc52_sequence_base64_encodes_utf8_text() {
        assert_eq!(osc52_sequence("hello"), "\x1b]52;c;aGVsbG8=\x07");
        assert_eq!(osc52_sequence("a你"), "\x1b]52;c;YeS9oA==\x07");
    }

    #[test]
    fn write_osc52_writes_and_flushes_sequence() {
        let mut out = Vec::new();

        write_osc52(&mut out, "copy me").expect("write OSC52");

        assert_eq!(out, b"\x1b]52;c;Y29weSBtZQ==\x07");
    }
}
