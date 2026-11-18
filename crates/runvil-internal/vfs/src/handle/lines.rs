//! A text read under line bounds: the whole file or a 1-based inclusive
//! range of its lines, plain or with each line behind its number.

use std::fmt::Write as _;

use super::Access;
use crate::error::VfsError;

/// Whether a text read prefixes each line with its absolute line number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LineNumbers {
    /// The file's text verbatim.
    Omitted,
    /// Each line right-aligned behind its number and `"| "`.
    Prefixed,
}

/// Reads the file at `path` through `access` under optional line bounds,
/// numbering the lines as `numbers` says.
///
/// No `start` reads the whole file; a present `start` slices a 1-based
/// inclusive line range. A negative bound converts to 0, which the range
/// validation rejects with the same error a zero bound produces, and an
/// `end` without a `start` is refused rather than silently ignored. A
/// positive bound that `usize` cannot hold counts as past the last line,
/// as every bound beyond the file does.
pub(super) fn read_lines(
    access: &Access,
    path: &str,
    start: Option<i64>,
    end: Option<i64>,
    numbers: LineNumbers,
) -> Result<String, VfsError> {
    let render: fn(&[&str], usize) -> String = match numbers {
        LineNumbers::Prefixed => number_lines_from,
        LineNumbers::Omitted => |lines, _| lines.join("\n"),
    };
    match start {
        None if end.is_some() => Err(VfsError::InvalidRange {
            path: path.to_owned(),
            reason: "start is required when end is given",
        }),
        None if numbers == LineNumbers::Omitted => access.read_string(path),
        None => with_read_range(access, path, 1, None, render),
        Some(start) => {
            let start = line_bound(start);
            let end = end.map(line_bound);
            with_read_range(access, path, start, end, render)
        }
    }
}

/// Converts a 1-based line bound to a `usize`. A negative bound becomes
/// 0, which the range validation rejects. A positive bound that `usize`
/// cannot hold, which a 32-bit target has, becomes `usize::MAX`, a line
/// past the end of every file.
fn line_bound(bound: i64) -> usize {
    bound_in(bound, usize::MAX)
}

/// Converts `bound` to the unsigned type of `most`, the largest value of
/// that type: a negative bound becomes zero, and a bound above `most`
/// becomes `most`. The type is a parameter so that a test on a 64-bit
/// target can run the conversion for a type of 32 bits.
fn bound_in<T: TryFrom<i64>>(bound: i64, most: T) -> T {
    T::try_from(bound.max(0)).unwrap_or(most)
}

/// Reads and resolves one line range while its owned contents remain live.
fn with_read_range(
    access: &Access,
    path: &str,
    start: usize,
    end: Option<usize>,
    render: impl FnOnce(&[&str], usize) -> String,
) -> Result<String, VfsError> {
    let contents = access.read_string(path)?;
    let lines: Vec<&str> = contents.lines().collect();
    let Some((start, end)) = resolve_line_range(path, lines.len(), start, end)? else {
        return Ok(String::new());
    };
    Ok(render(&lines[start - 1..end], start))
}

/// Resolves 1-based inclusive bounds against `line_count` into the
/// effective `(start, end)`, or `None` when the range falls entirely past
/// the last line. Evaluation order is fixed: a `start` below 1 is an
/// error; a `start` past the last line reads as empty; an omitted `end`
/// means the last line, and a given `end` clamps down to it; an `end`
/// before `start` at that point is an error.
fn resolve_line_range(
    path: &str,
    line_count: usize,
    start: usize,
    end: Option<usize>,
) -> Result<Option<(usize, usize)>, VfsError> {
    if start == 0 {
        return Err(VfsError::InvalidRange {
            path: path.to_owned(),
            reason: "start must be at least 1",
        });
    }
    if start > line_count {
        return Ok(None);
    }
    let end = end.unwrap_or(line_count).min(line_count);
    if end < start {
        return Err(VfsError::InvalidRange {
            path: path.to_owned(),
            reason: "end must not be before start",
        });
    }
    Ok(Some((start, end)))
}

/// Renders `lines` numbered absolutely from `start`, each number
/// right-aligned to the width of the largest emitted number, followed by
/// `"| "`; lines are joined with `"\n"` and there is no trailing newline.
fn number_lines_from(lines: &[&str], start: usize) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let last = start + lines.len() - 1;
    let width = last.to_string().len();
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let number = start + index;
        // Writing to a String is infallible, so the result is discarded.
        let _ = write!(out, "{number:>width$}| {line}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{bound_in, line_bound};

    #[test]
    fn a_negative_bound_is_zero_and_a_bound_too_large_for_the_type_is_its_largest_value() {
        assert_eq!(line_bound(i64::MIN), 0);
        assert_eq!(line_bound(-1), 0);
        assert_eq!(line_bound(0), 0);
        assert_eq!(line_bound(1), 1);
        assert_eq!(line_bound(4_096), 4_096);

        // The conversion for a `usize` of 32 bits, which a 32-bit target
        // has. No positive bound becomes 0 there, which the range
        // validation would reject.
        let narrow = |bound| bound_in(bound, u32::MAX);
        assert_eq!(narrow(-1), 0);
        assert_eq!(narrow(4_096), 4_096);
        assert_eq!(narrow(4_294_967_295), u32::MAX);
        assert_eq!(
            narrow(4_294_967_296),
            u32::MAX,
            "one past the largest value"
        );
        assert_eq!(narrow(i64::MAX), u32::MAX);
    }
}
