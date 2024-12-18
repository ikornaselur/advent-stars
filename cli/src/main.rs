use clap::Parser;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

/// CLI tool to generate SVG visualizations from Advent of Code stars data
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input file containing advent calendar data
    ///
    /// Input file should be a text file with a list of years and the 25 days for that year. The
    /// list of days should be 0, 1 or 2 where 0 is no stars, 1 is just part 1 and 2 means part 2
    /// has been solved.
    ///
    /// Example:
    ///
    /// 2024: 2,2,2,2,2,2,2,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0
    #[arg(help = "Path to the input .txt file")]
    input: PathBuf,

    /// Optional output file for the SVG (defaults to stdout if not provided)
    #[arg(short, long, help = "Optional path for the output SVG file")]
    output: Option<PathBuf>,

    /// Optional color override for primary stars
    #[arg(long, help = "Optional color override for primary stars")]
    primary_color: Option<String>,

    /// Optional color override for secondary stars
    #[arg(long, help = "Optional color override for secondary stars")]
    secondary_color: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let content =
        fs::read_to_string(&args.input).map_err(|e| format!("Failed to read input file: {}", e))?;

    let years = svg::validate_input(&content).map_err(|e| format!("Validation error: {:?}", e))?;

    let svg_content = svg::generate_svg(years, args.primary_color, args.secondary_color);

    match args.output {
        Some(path) => {
            fs::write(&path, svg_content)
                .map_err(|e| format!("Failed to write to output file: {}", e))?;
            println!("SVG successfully written to: {}", path.display());
        }
        None => {
            io::stdout()
                .write_all(svg_content.as_bytes())
                .map_err(|e| format!("Failed to write to stdout: {}", e))?;
        }
    }

    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::{tempdir, TempDir};

    // Keep the TempDir alive by returning it
    fn create_test_file(content: &str) -> (PathBuf, TempDir) {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test_input.txt");
        let mut file = File::create(&file_path).unwrap();
        write!(file, "{}", content).unwrap();
        (file_path, dir)
    }

    #[test]
    fn test_valid_input() {
        let content = "\
2023: 2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,1,0,0,0,0,0
2024: 2,2,2,2,2,2,2,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0";

        let (input_path, _input_dir) = create_test_file(content);
        let output_dir = tempdir().unwrap();
        let output_path = output_dir.path().join("output.svg");

        let args = Args {
            input: input_path,
            output: Some(output_path.clone()),
            primary_color: None,
            secondary_color: None,
        };

        let result: Result<(), String> = (|| {
            let content = fs::read_to_string(&args.input)
                .map_err(|e| format!("Failed to read input file: {}", e))?;

            let years = match svg::validate_input(&content) {
                Ok(y) => y,
                Err(e) => {
                    println!("Validation error occurred: {:?}", e);
                    return Err(format!("Validation error: {:?}", e));
                }
            };

            let svg_content = svg::generate_svg(years, args.primary_color.clone(), args.secondary_color.clone());

            if let Some(path) = args.output.as_ref() {
                fs::write(path, svg_content)
                    .map_err(|e| format!("Failed to write to output file: {}", e))?;
            }

            Ok(())
        })();

        if let Err(ref e) = result {
            println!("Test failed with error: {}", e);
        }
        assert!(result.is_ok(), "Test failed: {:?}", result.err().unwrap());
        assert!(output_path.exists());
        let output_content = fs::read_to_string(output_path).unwrap();
        assert!(output_content.contains("<svg"));
    }

    #[test]
    fn test_invalid_input() {
        let content = "invalid format";
        let (input_path, _input_dir) = create_test_file(content);

        let args = Args {
            input: input_path,
            output: None,
            primary_color: None,
            secondary_color: None,
        };

        let result: Result<(), String> = (|| {
            let content = fs::read_to_string(&args.input)
                .map_err(|e| format!("Failed to read input file: {}", e))?;

            let _years =
                svg::validate_input(&content).map_err(|e| format!("Validation error: {:?}", e))?;

            Ok(())
        })();

        assert!(result.is_err());
    }

    #[test]
    fn test_nonexistent_input_file() {
        let args = Args {
            input: PathBuf::from("nonexistent.txt"),
            output: None,
            primary_color: None,
            secondary_color: None,
        };

        let result: Result<(), String> = (|| {
            let _content = fs::read_to_string(&args.input)
                .map_err(|e| format!("Failed to read input file: {}", e))?;
            Ok(())
        })();

        assert!(result.is_err());
    }

    #[test]
    fn test_content_format() {
        let content = "\
2023: 2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,1,0,0,0,0,0
2024: 2,2,2,2,2,2,2,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0";

        println!("Testing content:\n{}", content);
        let result = svg::validate_input(content);
        println!("Validation result: {:?}", result);
        assert!(result.is_ok());
    }
}
