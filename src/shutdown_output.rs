//! Bounded, byte-preserving Windows shutdown-output filter.
//!
//! Only a contiguous trailing sequence of recognized diagnostics is eligible.
//! Location-bearing messages require their exact engine source/function pair.
//! Failed exits, incomplete messages and oversized tails are replayed unchanged.

use std::io::{self, Read, Write};

const MAX_LINE: usize = 8 * 1024;
const MAX_TAIL: usize = 64 * 1024;

#[derive(Clone, Copy)]
enum Location {
    Object,
    Resource,
    Allocator,
    Shader,
    RenderingDevice,
}

fn positive_count(text: &str) -> Option<&str> {
    let (count, rest) = text.split_once(' ')?;
    (count.bytes().all(|b| b.is_ascii_digit()) && count.parse::<u64>().ok()? > 0).then_some(rest)
}

fn type_name(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 1024
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_:<> *,".contains(&b))
}

// Some RID allocator diagnostics use print_line rather than ERR_PRINT, so they
// have no source-location continuation. The outer Option indicates recognition.
fn heading(line: &str) -> Option<Option<Location>> {
    if line == "WARNING: ObjectDB instances leaked at exit (run with --verbose for details)."
        || line
            == "WARNING: 1 ObjectDB instance was leaked at exit (run with `--verbose` for details)."
        || line.strip_prefix("WARNING: ").and_then(positive_count)
            == Some("ObjectDB instances were leaked at exit (run with `--verbose` for details).")
    {
        return Some(Some(Location::Object));
    }
    if let Some(rest) = line.strip_prefix("ERROR: ").and_then(positive_count) {
        if rest == "resources still in use at exit (run with --verbose for details)."
            || rest == "resources still in use at exit."
        {
            return Some(Some(Location::Resource));
        }
        if rest
            .strip_prefix("RID allocations of type '")
            .and_then(|s| s.strip_suffix("' were leaked at exit."))
            .is_some_and(type_name)
        {
            return Some(None);
        }
        if rest
            .strip_prefix("shaders of type ")
            .and_then(|s| s.strip_suffix(" were never freed"))
            .is_some_and(|s| type_name(s) && s.ends_with("ShaderRD"))
        {
            return Some(Some(Location::Shader));
        }
    }
    if line
        .strip_prefix("WARNING: ")
        .and_then(positive_count)
        .and_then(|s| s.strip_prefix("RIDs of type \""))
        .and_then(|s| s.strip_suffix("\" were leaked."))
        .is_some_and(|s| {
            matches!(
                s,
                "UniformBuffer"
                    | "StorageBuffer"
                    | "IndexArray"
                    | "IndexBuffer"
                    | "VertexArray"
                    | "VertexBuffer"
                    | "Texture"
                    | "Sampler"
                    | "Shader"
                    | "UniformSet"
                    | "RenderPipeline"
                    | "ComputePipeline"
                    | "Framebuffer"
            )
        })
    {
        return Some(Some(Location::RenderingDevice));
    }
    if line
        .strip_prefix("ERROR: Pages in use exist at exit in PagedAllocator: ")
        .is_some_and(type_name)
    {
        return Some(Some(Location::Allocator));
    }
    None
}

fn location_matches(line: &str, expected: Location) -> bool {
    let Some(site) = line
        .strip_prefix("   at: ")
        .and_then(|s| s.strip_suffix(')'))
    else {
        return false;
    };
    let Some((function, path)) = site.split_once(" (") else {
        return false;
    };
    let Some((path, number)) = path.rsplit_once(':') else {
        return false;
    };
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let path = path.replace('\\', "/");
    let path = path.strip_prefix("./").unwrap_or(&path);
    match expected {
        Location::Object => function == "cleanup" && path == "core/object/object.cpp",
        Location::Resource => function == "clear" && path == "core/io/resource.cpp",
        Location::Allocator => {
            function == "~PagedAllocator" && path == "core/templates/paged_allocator.h"
        }
        Location::Shader => {
            function == "~ShaderRD" && path == "servers/rendering/renderer_rd/shader_rd.cpp"
        }
        Location::RenderingDevice => {
            matches!(function, "_free_rids" | "finalize")
                && path == "servers/rendering/rendering_device.cpp"
        }
    }
}

#[derive(Default)]
pub(super) struct Tail {
    bytes: Vec<u8>,
    expected: Option<Location>,
    count: usize,
    disabled: bool,
}

impl Tail {
    fn replay(&mut self, output: &mut impl Write) -> io::Result<()> {
        output.write_all(&self.bytes)?;
        self.bytes.clear();
        self.expected = None;
        self.count = 0;
        Ok(())
    }

    fn line(&mut self, bytes: &[u8], output: &mut impl Write) -> io::Result<()> {
        let text = std::str::from_utf8(bytes)
            .ok()
            .and_then(|s| s.strip_suffix('\n'))
            .map(|s| s.strip_suffix('\r').unwrap_or(s));
        if !self.disabled {
            if let (Some(expected), Some(text)) = (self.expected, text) {
                if location_matches(text, expected) {
                    self.expected = None;
                    self.bytes.extend_from_slice(bytes);
                    return self.check_limit(output);
                }
                // An incomplete record cannot be hidden by a subsequent heading.
                self.replay(output)?;
            }
            if let Some(location) = text.and_then(heading) {
                self.expected = location;
                self.count += 1;
                self.bytes.extend_from_slice(bytes);
                return self.check_limit(output);
            }
        }
        self.replay(output)?;
        output.write_all(bytes)?;
        output.flush()
    }

    fn check_limit(&mut self, output: &mut impl Write) -> io::Result<()> {
        if self.bytes.len() > MAX_TAIL {
            self.disabled = true;
            self.replay(output)?;
            output.flush()?;
        }
        Ok(())
    }

    pub(super) fn finish(mut self, success: bool, mut output: impl Write) -> io::Result<usize> {
        let count = if success && self.expected.is_none() && !self.disabled {
            self.count
        } else {
            self.replay(&mut output)?;
            0
        };
        output.flush()?;
        Ok(count)
    }
}

pub(super) fn drain(mut input: impl Read, mut output: impl Write) -> io::Result<Tail> {
    let mut tail = Tail::default();
    let mut pending = Vec::new();
    let mut chunk = [0; 4096];
    let mut long_line = false;
    let result = (|| {
        loop {
            let size = match input.read(&mut chunk) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            if size == 0 {
                break;
            }
            for &byte in &chunk[..size] {
                pending.push(byte);
                if byte == b'\n' {
                    if long_line {
                        output.write_all(&pending)?;
                        output.flush()?;
                    } else {
                        tail.line(&pending, &mut output)?;
                    }
                    pending.clear();
                    long_line = false;
                } else if pending.len() >= MAX_LINE {
                    tail.replay(&mut output)?;
                    output.write_all(&pending)?;
                    output.flush()?;
                    pending.clear();
                    long_line = true;
                }
            }
        }
        if !pending.is_empty() {
            tail.replay(&mut output)?;
            output.write_all(&pending)?;
            output.flush()?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        // Preserve pending evidence on read failure. On output failure keep
        // draining so the child cannot deadlock waiting for its pipe reader.
        let _ = tail.replay(&mut output);
        let _ = output.write_all(&pending);
        let _ = io::copy(&mut input, &mut io::sink());
        return Err(error);
    }
    Ok(tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTURE: &[u8] = include_bytes!("../tests/fixtures/windows-shutdown.txt");

    fn filtered(input: &[u8], success: bool) -> (Vec<u8>, usize) {
        let mut output = Vec::new();
        let tail = drain(input, &mut output).unwrap();
        let count = tail.finish(success, &mut output).unwrap();
        (output, count)
    }

    #[test]
    fn captured_windows_shutdown_is_filtered_only_on_success() {
        assert_eq!(filtered(CAPTURE, true), (Vec::new(), 15));
        assert_eq!(filtered(CAPTURE, false), (CAPTURE.to_vec(), 0));
        let crlf = String::from_utf8(CAPTURE.to_vec())
            .unwrap()
            .replace("\r\n", "\n")
            .replace('\n', "\r\n");
        assert_eq!(filtered(crlf.as_bytes(), true), (Vec::new(), 15));
    }

    #[test]
    fn unrelated_output_and_non_trailing_diagnostics_survive_byte_for_byte() {
        let mut input = b"progress\r\n\xff\xfe\n".to_vec();
        input.extend(CAPTURE);
        input.extend(b"SCRIPT ERROR: runtime failed\n   at: cleanup (game.gd:10)\n");
        assert_eq!(filtered(&input, true), (input.clone(), 0));
        let mut with_tail = input.clone();
        with_tail.extend(CAPTURE);
        assert_eq!(filtered(&with_tail, true), (input, 15));
    }

    #[test]
    fn incomplete_or_unknown_locations_are_not_filtered() {
        for input in [
            "WARNING: ObjectDB instances leaked at exit (run with --verbose for details).\n",
            "ERROR: 2 resources still in use at exit.\n   at: clear (game.gd:822)\n",
            "ERROR: 0 RID allocations of type 'Mesh' were leaked at exit.\n",
            "ERROR: 1 RID allocations of type 'Mesh' were leaked at exit.",
            "ERROR: 1 RID allocations of type 'Mesh' were leaked at exit. extra\n",
        ] {
            assert_eq!(
                filtered(input.as_bytes(), true),
                (input.as_bytes().to_vec(), 0)
            );
        }
    }

    #[test]
    fn long_lines_and_tail_overflow_are_bounded_and_preserved() {
        let mut input = vec![b'x'; MAX_LINE * 3];
        input.extend(CAPTURE); // First heading is part of the overlong line.
        assert!(
            filtered(&input, true)
                .0
                .starts_with(&vec![b'x'; MAX_LINE * 3])
        );
        let input = CAPTURE.repeat(MAX_TAIL / CAPTURE.len() + 2);
        assert_eq!(filtered(&input, true), (input.clone(), 0));
        assert_eq!(filtered(&input, false), (input, 0));
    }

    #[test]
    fn split_reads_and_legacy_object_warning_are_supported() {
        struct OneByte<'a>(&'a [u8]);
        impl Read for OneByte<'_> {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                self.0.read(&mut bytes[..1])
            }
        }
        let mut output = Vec::new();
        let tail = drain(OneByte(CAPTURE), &mut output).unwrap();
        assert_eq!(tail.finish(true, &mut output).unwrap(), 15);
        assert!(output.is_empty());
        let legacy = b"WARNING: ObjectDB instances leaked at exit (run with --verbose for details).\n   at: cleanup (core/object/object.cpp:2378)\n";
        assert_eq!(filtered(legacy, true), (Vec::new(), 1));
        let singular = b"WARNING: 1 ObjectDB instance was leaked at exit (run with `--verbose` for details).\r\n   at: cleanup (core\\object\\object.cpp:2535)\r\n";
        assert_eq!(filtered(singular, true), (Vec::new(), 1));
        assert_eq!(filtered(singular, false), (singular.to_vec(), 0));
    }

    #[test]
    fn read_failure_replays_pending_diagnostics() {
        struct FailingRead<'a>(&'a [u8]);
        impl Read for FailingRead<'_> {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                if self.0.is_empty() {
                    Err(io::Error::other("reader failed"))
                } else {
                    self.0.read(bytes)
                }
            }
        }
        let mut output = Vec::new();
        assert!(drain(FailingRead(CAPTURE), &mut output).is_err());
        assert_eq!(output, CAPTURE);
    }

    #[test]
    fn output_failure_still_drains_input() {
        struct FailingWrite;
        impl Write for FailingWrite {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let input = b"ordinary output\n".repeat(1024);
        let mut reader = io::Cursor::new(&input);
        assert!(drain(&mut reader, FailingWrite).is_err());
        assert_eq!(reader.position(), input.len() as u64);
    }
}
