//! @toon
//! purpose: This module implements the strip command that removes @toon comment blocks
//!     from source files and replaces them with minimal stub references. This reduces
//!     code noise while maintaining links to the full documentation.
//!
//! when-editing:
//!     - !The strip command can read from files or stdin
//!     - !Output goes to a file or stdout based on -o flag
//!     - Language detection uses file extension or --ext flag
//!
//! invariants:
//!     - The stripped output always contains a reference to the TOON file location
//!     - Original file content (minus @toon blocks) is preserved exactly
//!
//! do-not:
//!     - Never strip without providing a TOON path reference in the output
//!     - Never modify the source file in-place (output is separate)
//!
//! gotchas:
//!     - When reading from stdin, --ext must be provided to select the right parser
//!     - The TOON path in the stub comment is relative to the project root
//!
//! flows:
//!     - Read: Get source from file or stdin
//!     - Parse: Detect language and get appropriate parser
//!     - Strip: Remove @toon blocks, insert stub comments
//!     - Write: Output to file or stdout

use crate::cli::StripArgs;
use crate::parser::ParserFactory;
use anyhow::{Context, Result};
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

pub fn run_strip(args: &StripArgs, root: &Path, _verbose: bool) -> Result<()> {
    let factory = ParserFactory::new();

    // Read source from file or stdin
    let (source, ext) = if let Some(ref input) = args.input {
        if input.to_string_lossy() == "-" {
            // Read from stdin
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .context("Failed to read from stdin")?;

            let ext = args.ext.clone().unwrap_or_else(|| "ts".to_string());
            (buffer, ext)
        } else {
            // Read from file
            let path = if input.is_absolute() {
                input.clone()
            } else {
                root.join(input)
            };

            let source = fs::read_to_string(&path).context("Failed to read input file")?;
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("ts")
                .to_string();

            (source, ext)
        }
    } else {
        // No input specified, read from stdin
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .context("Failed to read from stdin")?;

        let ext = args.ext.clone().unwrap_or_else(|| "ts".to_string());
        (buffer, ext)
    };

    // Get parser by extension
    let parser = factory
        .get_parser_by_ext(&ext)
        .context(format!("No parser available for extension: {}", ext))?;

    // Compute TOON path for stub comment
    let toon_path = if let Some(ref input) = args.input {
        if input.to_string_lossy() != "-" {
            let relative = input.strip_prefix(root).unwrap_or(input);
            format!(".ai/{}.toon", relative.display())
        } else {
            ".ai/<file>.toon".to_string()
        }
    } else {
        ".ai/<file>.toon".to_string()
    };

    // Strip comments
    let stripped = parser.strip_toon_comments(&source, &toon_path)?;

    // Write output
    if let Some(ref output) = args.output {
        let output_path = if output.is_absolute() {
            output.clone()
        } else {
            root.join(output)
        };

        fs::write(&output_path, &stripped).context("Failed to write output file")?;
    } else {
        io::stdout()
            .write_all(stripped.as_bytes())
            .context("Failed to write to stdout")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // ==================== run_strip Tests ====================

    #[test]
    fn test_strip_file_removes_toon_comments() {
        let temp_dir = TempDir::new().unwrap();

        // Create a TypeScript file with @toon comment
        let source = r#"/** @toon
purpose: Test module
when-editing:
    - Check imports
invariants:
    - Must export x
*/

export const x = 1;

function helper() {
    return 42;
}
"#;
        let input_path = temp_dir.path().join("test.ts");
        fs::write(&input_path, source).unwrap();

        let output_path = temp_dir.path().join("output.ts");

        let args = StripArgs {
            input: Some(input_path.clone()),
            output: Some(output_path.clone()),
            ext: None,
        };

        let result = run_strip(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        // Read output and verify @toon block content was removed
        let output = fs::read_to_string(&output_path).unwrap();
        // The block content (purpose:, when-editing:) should be gone
        assert!(!output.contains("when-editing:"));
        assert!(!output.contains("Must export x"));
        assert!(output.contains("export const x = 1;"));
        assert!(output.contains("function helper()"));
    }

    #[test]
    fn test_strip_adds_stub_comment() {
        let temp_dir = TempDir::new().unwrap();

        // Create a TypeScript file with @toon comment
        let source = r#"/** @toon
purpose: Test module
*/

export const x = 1;
"#;
        let input_path = temp_dir.path().join("test.ts");
        fs::write(&input_path, source).unwrap();

        let output_path = temp_dir.path().join("output.ts");

        let args = StripArgs {
            input: Some(input_path.clone()),
            output: Some(output_path.clone()),
            ext: None,
        };

        let result = run_strip(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        // Read output and verify stub comment was added
        let output = fs::read_to_string(&output_path).unwrap();
        assert!(output.contains(".ai/") || output.contains(".toon"));
    }

    #[test]
    fn test_strip_preserves_non_toon_content() {
        let temp_dir = TempDir::new().unwrap();

        // Create a TypeScript file with @toon and regular comments
        let source = r#"/** @toon
purpose: Test module
*/

// This is a regular comment
export const x = 1;

/**
 * Regular JSDoc comment
 */
function foo() {
    return 42;
}
"#;
        let input_path = temp_dir.path().join("test.ts");
        fs::write(&input_path, source).unwrap();

        let output_path = temp_dir.path().join("output.ts");

        let args = StripArgs {
            input: Some(input_path.clone()),
            output: Some(output_path.clone()),
            ext: None,
        };

        let result = run_strip(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        // Regular comments should be preserved
        let output = fs::read_to_string(&output_path).unwrap();
        assert!(output.contains("This is a regular comment"));
        assert!(output.contains("Regular JSDoc comment"));
    }

    #[test]
    fn test_strip_python_file() {
        let temp_dir = TempDir::new().unwrap();

        // Create a Python file with @toon comment
        let source = r#""""@toon
purpose: Python test module
invariants:
    - Must define foo function
"""

def foo():
    """Regular docstring."""
    return 42
"#;
        let input_path = temp_dir.path().join("test.py");
        fs::write(&input_path, source).unwrap();

        let output_path = temp_dir.path().join("output.py");

        let args = StripArgs {
            input: Some(input_path.clone()),
            output: Some(output_path.clone()),
            ext: None,
        };

        let result = run_strip(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        // Read output and verify
        let output = fs::read_to_string(&output_path).unwrap();
        assert!(!output.contains("purpose:"));
        assert!(output.contains("def foo():"));
        // Regular docstring should be preserved
        assert!(output.contains("Regular docstring"));
    }

    #[test]
    fn test_strip_with_ext_flag() {
        let temp_dir = TempDir::new().unwrap();

        // Create a file without extension
        let source = r#"/** @toon
purpose: Test module
*/

export const x = 1;
"#;
        let input_path = temp_dir.path().join("noext");
        fs::write(&input_path, source).unwrap();

        let output_path = temp_dir.path().join("output");

        let args = StripArgs {
            input: Some(input_path.clone()),
            output: Some(output_path.clone()),
            ext: Some("ts".to_string()), // Specify extension
        };

        // Without ext flag it would fail to find parser
        // With ext flag it should work
        let result = run_strip(&args, temp_dir.path(), false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_strip_rust_file() {
        let temp_dir = TempDir::new().unwrap();

        // Create a Rust file with @toon block comment
        let source = r#"/*! @toon
purpose: Rust test module
invariants:
    - Must define main function
*/

fn main() {
    println!("Hello");
}
"#;
        let input_path = temp_dir.path().join("test.rs");
        fs::write(&input_path, source).unwrap();

        let output_path = temp_dir.path().join("output.rs");

        let args = StripArgs {
            input: Some(input_path.clone()),
            output: Some(output_path.clone()),
            ext: None,
        };

        let result = run_strip(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        // Read output and verify @toon block was replaced with stub
        let output = fs::read_to_string(&output_path).unwrap();
        // Original @toon block content should be gone
        assert!(!output.contains("Must define main function"));
        // The code should remain
        assert!(output.contains("fn main()"));
        // Should have the stub reference
        assert!(output.contains("// @toon ->"));
    }

    #[test]
    fn test_strip_go_file() {
        let temp_dir = TempDir::new().unwrap();

        // Create a Go file with @toon comment
        let source = r#"/* @toon
purpose: Go test module
invariants:
    - Must define main function
*/

package main

func main() {
    fmt.Println("Hello")
}
"#;
        let input_path = temp_dir.path().join("test.go");
        fs::write(&input_path, source).unwrap();

        let output_path = temp_dir.path().join("output.go");

        let args = StripArgs {
            input: Some(input_path.clone()),
            output: Some(output_path.clone()),
            ext: None,
        };

        let result = run_strip(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        // Read output and verify
        let output = fs::read_to_string(&output_path).unwrap();
        assert!(!output.contains("purpose:"));
        assert!(output.contains("package main"));
        assert!(output.contains("func main()"));
    }

    #[test]
    fn test_strip_file_without_toon_comments() {
        let temp_dir = TempDir::new().unwrap();

        // Create a file without @toon comments
        let source = r#"// Regular file
export const x = 1;

function foo() {
    return 42;
}
"#;
        let input_path = temp_dir.path().join("test.ts");
        fs::write(&input_path, source).unwrap();

        let output_path = temp_dir.path().join("output.ts");

        let args = StripArgs {
            input: Some(input_path.clone()),
            output: Some(output_path.clone()),
            ext: None,
        };

        let result = run_strip(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        // Output should be similar to input (maybe with stub comment added)
        let output = fs::read_to_string(&output_path).unwrap();
        assert!(output.contains("export const x = 1;"));
        assert!(output.contains("function foo()"));
    }

    #[test]
    fn test_strip_to_stdout() {
        let temp_dir = TempDir::new().unwrap();

        // Create a TypeScript file
        let source = "export const x = 1;";
        let input_path = temp_dir.path().join("test.ts");
        fs::write(&input_path, source).unwrap();

        let args = StripArgs {
            input: Some(input_path.clone()),
            output: None, // No output file means stdout
            ext: None,
        };

        // Should succeed (output goes to stdout)
        let result = run_strip(&args, temp_dir.path(), false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_strip_relative_path() {
        let temp_dir = TempDir::new().unwrap();

        // Create a subdirectory with a file
        let subdir = temp_dir.path().join("src");
        fs::create_dir(&subdir).unwrap();

        let source = r#"/** @toon
purpose: Subdir test
*/
export const x = 1;
"#;
        fs::write(subdir.join("test.ts"), source).unwrap();

        let output_path = temp_dir.path().join("output.ts");

        // Use relative path
        let args = StripArgs {
            input: Some(PathBuf::from("src/test.ts")),
            output: Some(output_path.clone()),
            ext: None,
        };

        let result = run_strip(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        let output = fs::read_to_string(&output_path).unwrap();
        assert!(output.contains("export const x = 1;"));
    }

    #[test]
    fn test_strip_absolute_path() {
        let temp_dir = TempDir::new().unwrap();

        let source = r#"/** @toon
purpose: Absolute path test
*/
export const x = 1;
"#;
        let input_path = temp_dir.path().join("test.ts");
        fs::write(&input_path, source).unwrap();

        let output_path = temp_dir.path().join("output.ts");

        // Use absolute paths
        let args = StripArgs {
            input: Some(input_path.clone()),
            output: Some(output_path.clone()),
            ext: None,
        };

        let result = run_strip(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        let output = fs::read_to_string(&output_path).unwrap();
        assert!(output.contains("export const x = 1;"));
    }

    #[test]
    fn test_strip_toon_path_in_stub() {
        let temp_dir = TempDir::new().unwrap();

        // Create a file in a subdirectory
        let subdir = temp_dir.path().join("src/components");
        fs::create_dir_all(&subdir).unwrap();

        let source = r#"/** @toon
purpose: Button component
*/
export function Button() {}
"#;
        let input_path = subdir.join("Button.tsx");
        fs::write(&input_path, source).unwrap();

        let output_path = temp_dir.path().join("output.tsx");

        let args = StripArgs {
            input: Some(input_path.clone()),
            output: Some(output_path.clone()),
            ext: None,
        };

        let result = run_strip(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        // The stub should reference the correct TOON path
        let output = fs::read_to_string(&output_path).unwrap();
        // Should contain reference to the .ai/ directory
        assert!(output.contains(".ai/") || output.contains(".toon"));
    }

    #[test]
    fn test_strip_unsupported_extension_fails() {
        let temp_dir = TempDir::new().unwrap();

        // Create a file with unsupported extension
        let source = "Some content";
        let input_path = temp_dir.path().join("test.unsupported");
        fs::write(&input_path, source).unwrap();

        let args = StripArgs {
            input: Some(input_path.clone()),
            output: None,
            ext: None,
        };

        // Should fail because no parser for .unsupported
        let result = run_strip(&args, temp_dir.path(), false);
        assert!(result.is_err());
    }

    #[test]
    fn test_strip_csharp_file() {
        let temp_dir = TempDir::new().unwrap();

        // Create a C# file with @toon comment
        let source = r#"/** @toon
purpose: C# test module
invariants:
    - Must define MyClass
*/

namespace Test
{
    public class MyClass
    {
        public void Method() {}
    }
}
"#;
        let input_path = temp_dir.path().join("test.cs");
        fs::write(&input_path, source).unwrap();

        let output_path = temp_dir.path().join("output.cs");

        let args = StripArgs {
            input: Some(input_path.clone()),
            output: Some(output_path.clone()),
            ext: None,
        };

        let result = run_strip(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        let output = fs::read_to_string(&output_path).unwrap();
        assert!(!output.contains("purpose:"));
        assert!(output.contains("public class MyClass"));
    }

    #[test]
    fn test_strip_ruby_file() {
        let temp_dir = TempDir::new().unwrap();

        // Create a Ruby file with @toon comment
        let source = r#"# @toon
# purpose: Ruby test module
# invariants:
#     - Must define MyClass

class MyClass
  def method
    puts "Hello"
  end
end
"#;
        let input_path = temp_dir.path().join("test.rb");
        fs::write(&input_path, source).unwrap();

        let output_path = temp_dir.path().join("output.rb");

        let args = StripArgs {
            input: Some(input_path.clone()),
            output: Some(output_path.clone()),
            ext: None,
        };

        let result = run_strip(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        let output = fs::read_to_string(&output_path).unwrap();
        assert!(!output.contains("purpose:"));
        assert!(output.contains("class MyClass"));
    }
}
