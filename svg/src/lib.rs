mod validation;

pub use validation::validate_input;

const CELL_SIZE: i32 = 20;
const FONT_SIZE: i32 = 12;
const X_OFFSET: i32 = 40;
const Y_OFFSET: i32 = 60;
const YEAR_Y_OFFSET: i32 = 5;
const PADDING: i32 = 20;
const MATRIX_BORDER: i32 = 1;

type Year = (usize, Vec<u8>);
type Years = Vec<Year>;

#[derive(Copy, Clone)]
enum Star {
    None = 0,
    Silver = 1,
    Gold = 2,
}

impl From<u8> for Star {
    fn from(value: u8) -> Self {
        match value {
            1 => Star::Silver,
            2 => Star::Gold,
            _ => Star::None,
        }
    }
}

struct SvgBuilder {
    content: String,
    width: i32,
    height: i32,
    matrix_width: i32,
    matrix_height: i32,
}

impl SvgBuilder {
    fn new(num_days: i32, num_years: i32) -> Self {
        let matrix_width = (num_days + 1) * CELL_SIZE;
        let matrix_height = num_years * CELL_SIZE;
        let width = X_OFFSET + matrix_width + PADDING * 2;
        let height = Y_OFFSET + matrix_height + PADDING * 4;

        let mut builder = Self {
            content: String::new(),
            width,
            height,
            matrix_width,
            matrix_height,
        };

        builder.add_header();
        builder
    }

    fn add_header(&mut self) {
        self.content.push_str(&format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#,
            self.width, self.height, self.width, self.height
        ));
        self.content.push_str(
            r#"
            <style>
                @media (prefers-color-scheme: light) {
                    .text { fill: #24292f; }
                    .grid-line { stroke: #24292f; }
                    .matrix-border { stroke: #24292f; }
                }
                @media (prefers-color-scheme: dark) {
                    .text { fill: #c9d1d9; }
                    .grid-line { stroke: #c9d1d9; }
                    .matrix-border { stroke: #c9d1d9; }
                }
                .year-label { font-family: Arial; font-size: 12px; }
                .day-label { font-family: Arial; font-size: 12px; }
                .total-label { font-family: Arial; font-size: 12px; font-weight: bold; }
                .grand-total { font-family: Arial; font-size: 14px; font-weight: bold; }
                .star { font-family: Arial; font-size: 12px; }
                .silver { fill: #6b7280; }
                .gold { fill: #fbbf24; }
                .matrix-border { fill: none; stroke-width: 1; }
                .grid-line { stroke-width: 0.5; stroke-opacity: 0.1; }
                .text { font-family: Arial; }
            </style>"#,
        );

        self.content.push_str(&format!(
            r#"<rect x="{}" y="{}" width="{}" height="{}" class="matrix-border"/>"#,
            X_OFFSET - MATRIX_BORDER,
            Y_OFFSET - MATRIX_BORDER,
            self.matrix_width + MATRIX_BORDER * 2,
            self.matrix_height + MATRIX_BORDER * 2
        ));
    }

    fn add_grid(&mut self, num_days: i32, num_years: i32) {
        for i in 0..=(num_days + 1) {
            let x = X_OFFSET + i * CELL_SIZE;
            self.content.push_str(&format!(
                r#"<line x1="{}" y1="{}" x2="{}" y2="{}" class="grid-line"/>"#,
                x,
                Y_OFFSET,
                x,
                Y_OFFSET + self.matrix_height
            ));
        }

        for i in 0..=num_years {
            let y = Y_OFFSET + i * CELL_SIZE;
            self.content.push_str(&format!(
                r#"<line x1="{}" y1="{}" x2="{}" y2="{}" class="grid-line"/>"#,
                X_OFFSET,
                y,
                X_OFFSET + self.matrix_width,
                y
            ));
        }
    }

    fn add_year_labels(&mut self, years: &[usize]) {
        for (i, year) in years.iter().enumerate() {
            let y_position = Y_OFFSET + YEAR_Y_OFFSET + (i as i32) * CELL_SIZE;
            self.content.push_str(&format!(
                r#"<text x="{}" y="{}" class="year-label text" text-anchor="end">{}</text>"#,
                X_OFFSET - PADDING / 2,
                y_position + CELL_SIZE / 2,
                year,
            ));
        }
    }

    fn add_day_labels(&mut self, num_days: i32) {
        for day in 0..num_days {
            let x_position = X_OFFSET + day * CELL_SIZE;
            let day_num = day + 1;

            if day_num < 10 {
                self.content.push_str(&format!(
                    r#"<text x="{}" y="{}" class="day-label text" text-anchor="middle">{}</text>"#,
                    x_position + CELL_SIZE / 2,
                    Y_OFFSET - PADDING / 4,
                    day_num
                ));
            } else {
                let tens = day_num / 10;
                let ones = day_num % 10;
                self.content.push_str(&format!(
                    r#"<text x="{}" y="{}" class="day-label text" text-anchor="middle">{}</text>
                    <text x="{}" y="{}" class="day-label text" text-anchor="middle">{}</text>"#,
                    x_position + CELL_SIZE / 2,
                    Y_OFFSET - PADDING - 2,
                    tens,
                    x_position + CELL_SIZE / 2,
                    Y_OFFSET - PADDING / 4,
                    ones
                ));
            }
        }
    }

    fn add_stars(&mut self, years: &Years) {
        let mut grand_total = 0;

        for (i, (_, days)) in years.iter().enumerate() {
            let y_position = Y_OFFSET + i as i32 * CELL_SIZE;
            let mut year_total = 0;

            for (day_index, &value) in days.iter().enumerate() {
                let star: Star = value.into();
                if matches!(star, Star::None) {
                    continue;
                }

                year_total += value as i32;
                let x_position = X_OFFSET + day_index as i32 * CELL_SIZE;
                let star_class = match star {
                    Star::Silver => "silver",
                    Star::Gold => "gold",
                    Star::None => continue,
                };

                self.content.push_str(&format!(
                    r#"<text x="{}" y="{}" class="star {}" text-anchor="middle">★</text>"#,
                    x_position + CELL_SIZE / 2,
                    y_position + CELL_SIZE / 2 + FONT_SIZE / 3,
                    star_class
                ));
            }

            let total_x = X_OFFSET + days.len() as i32 * CELL_SIZE;
            self.content.push_str(&format!(
                r#"<text x="{}" y="{}" class="total-label text" text-anchor="middle">{}</text>"#,
                total_x + CELL_SIZE / 2,
                y_position + CELL_SIZE / 2 + FONT_SIZE / 3,
                year_total
            ));

            grand_total += year_total;
        }

        let center_x = X_OFFSET + self.matrix_width / 2;
        let total_y = Y_OFFSET + self.matrix_height + PADDING * 2;
        self.content.push_str(&format!(
            r#"<text x="{}" y="{}" class="grand-total text" text-anchor="middle">Total stars: {}</text>"#,
            center_x,
            total_y,
            grand_total
        ));
    }

    fn finalize(mut self) -> String {
        self.content.push_str("</svg>");
        self.content
    }
}

pub fn generate_svg(years: Years) -> String {
    let num_years = years.len() as i32;
    let num_days = years.first().map_or(0, |(_, days)| days.len()) as i32;

    let mut builder = SvgBuilder::new(num_days, num_years);
    builder.add_grid(num_days, num_years);
    builder.add_year_labels(&years.iter().map(|(year, _)| *year).collect::<Vec<_>>());
    builder.add_day_labels(num_days);
    builder.add_stars(&years);
    builder.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_years() {
        let years: Years = vec![];
        let svg = generate_svg(years);
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn test_single_year_no_stars() {
        let years: Years = vec![(2023, vec![0; 25])];
        let svg = generate_svg(years);
        assert!(svg.contains("2023"));
        assert!(svg.contains("Total stars: 0"));
    }

    #[test]
    fn test_silver_and_gold_stars() {
        let years: Years = vec![(2023, vec![0, 1, 2, 0, 1])];
        let svg = generate_svg(years);

        // Check for silver star
        assert!(svg.contains(r#"class="star silver"#));
        // Check for gold star
        assert!(svg.contains(r#"class="star gold"#));
        // Check total (1 + 2 + 1 = 4 points)
        assert!(svg.contains("Total stars: 4"));
    }

    #[test]
    fn test_multiple_years() {
        let years: Years = vec![(2022, vec![1, 1]), (2023, vec![2, 2])];
        let svg = generate_svg(years);

        // Check year labels
        assert!(svg.contains("2022"));
        assert!(svg.contains("2023"));

        // Check totals (2 silver = 2, 2 gold = 4, total = 6)
        assert!(svg.contains("Total stars: 6"));
    }

    #[test]
    fn test_star_conversion() {
        assert!(matches!(Star::from(0), Star::None));
        assert!(matches!(Star::from(1), Star::Silver));
        assert!(matches!(Star::from(2), Star::Gold));
        assert!(matches!(Star::from(3), Star::None)); // Invalid value should become None
    }

    #[test]
    fn test_style_definitions() {
        let years: Years = vec![(2023, vec![0; 1])];
        let svg = generate_svg(years);

        // Check for style definitions
        assert!(svg.contains("<style>"));
        assert!(svg.contains("prefers-color-scheme: light"));
        assert!(svg.contains("prefers-color-scheme: dark"));
        assert!(svg.contains(".silver {"));
        assert!(svg.contains(".gold {"));
    }

    // Helper function to count occurrences of a pattern in a string
    fn count_occurrences(haystack: &str, needle: &str) -> usize {
        haystack.matches(needle).count()
    }

    #[test]
    fn test_correct_star_counts() {
        let years: Years = vec![
            (2023, vec![1, 2, 1, 0, 2]), // 2 silver (1+1) and 2 gold (2+2) = 6 total
        ];
        let svg = generate_svg(years);

        let silver_stars = count_occurrences(&svg, r#"class="star silver"#);
        let gold_stars = count_occurrences(&svg, r#"class="star gold"#);

        assert_eq!(silver_stars, 2, "Should have exactly 2 silver stars");
        assert_eq!(gold_stars, 2, "Should have exactly 2 gold stars");
        assert!(svg.contains("Total stars: 6"));
    }
}
