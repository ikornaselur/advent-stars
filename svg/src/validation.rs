use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum ValidationError {
    EmptyInput,
    InvalidLineFormat { line: usize, content: String },
    InvalidYear { line: usize, year: String },
    InvalidDayCount { year: usize, count: usize },
    InvalidStarValue { year: usize },
    ParseError { year: usize, error: String },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "No valid data found in input"),
            Self::InvalidLineFormat { line, content } => {
                write!(f, "Invalid line format on line {}: {}", line, content)
            }
            Self::InvalidYear { line, year } => {
                write!(f, "Invalid year on line {}: {}", line, year)
            }
            Self::InvalidDayCount { year, count } => {
                write!(f, "Year {} has {} days, expected 25", year, count)
            }
            Self::InvalidStarValue { year } => {
                write!(f, "Year {} has invalid stars (must be 0, 1, or 2)", year)
            }
            Self::ParseError { year, error } => {
                write!(f, "Error parsing year {}: {}", year, error)
            }
        }
    }
}

impl Error for ValidationError {}

pub fn validate_input(content: &str) -> Result<Vec<(usize, Vec<u8>)>, ValidationError> {
    let mut years = Vec::new();

    for (i, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(ValidationError::InvalidLineFormat {
                line: i + 1,
                content: line.to_string(),
            });
        }

        let year = parts[0]
            .trim()
            .parse::<usize>()
            .map_err(|_| ValidationError::InvalidYear {
                line: i + 1,
                year: parts[0].to_string(),
            })?;

        let days: Vec<u8> = parts[1]
            .trim()
            .split(',')
            .map(|s| s.trim().parse::<u8>())
            .collect::<Result<_, _>>()
            .map_err(|err| ValidationError::ParseError {
                year,
                error: err.to_string(),
            })?;

        if days.len() != 25 {
            return Err(ValidationError::InvalidDayCount {
                year,
                count: days.len(),
            });
        }

        if days.iter().any(|&d| d > 2) {
            return Err(ValidationError::InvalidStarValue { year });
        }

        years.push((year, days));
    }

    if years.is_empty() {
        return Err(ValidationError::EmptyInput);
    }

    Ok(years)
}

// Optional: Add a test module
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_input() {
        let input = "2015: 2,2,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0\n\
                     2016: 2,2,2,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0";
        assert!(validate_input(input).is_ok());
    }

    #[test]
    fn test_invalid_star_value() {
        let input = "2015: 3,2,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0";
        assert!(matches!(
            validate_input(input),
            Err(ValidationError::InvalidStarValue { .. })
        ));
    }
}
