// Visible-ranges targeting test fixture.
// This file deliberately exercises the shapes that matter for
// GetVisibleRanges line targeting correctness.

/// A short line with a doc comment.
fn short_line() -> bool {
    true
}

/// A longer line that may or may not wrap depending on the editor width and font size settings chosen by the reader.
fn long_line() -> &'static str {
    "result should remain pending"
}

// Punctuation: commas, semicolons, and brackets all appear here; the caret may rest on one.
fn punctuation() -> char {
    ';'
}

fn duplicate_word_test() {
    // result should remain pending
    let result = "first";
    // result should remain visible
    let other = "second";
    let _ = (result, other);
}

fn indentation_test() {
    if true {
        if true {
            if true {
                let deeply_nested = 42;
                let _ = deeply_nested;
            }
        }
    }
}

// A blank line follows this comment.

fn after_blank() -> i32 {
    0
}

/* A block comment spanning
   multiple lines inside the
   fixture file. */
fn after_block_comment() {}
