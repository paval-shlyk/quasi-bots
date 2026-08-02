use clap::{Parser, ValueEnum};
use finance::expenses::report::{MonthlyReport, WeeklyReport, YearReport};
use finance::expenses::{create_bar_chart, create_year_chart};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "report-cli")]
#[command(about = "Generates PNG charts from expense report JSON", long_about = None)]
struct Cli {
    /// Input JSON file path
    #[arg(short, long)]
    input: PathBuf,

    /// Output PNG file path
    #[arg(short, long)]
    output: PathBuf,

    /// Report type
    #[arg(short, long, value_enum)]
    type_: ReportType,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum ReportType {
    Monthly,
    Weekly,
    Yearly,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let json_content = fs::read_to_string(&cli.input)?;

    let png_data = match cli.type_ {
        ReportType::Monthly => {
            let report: MonthlyReport = serde_json::from_str(&json_content)?;
            let title = format!("Expenses {}/{}", report.year, report.month);
            create_bar_chart(&report.by_category, &title)?
        }
        ReportType::Weekly => {
            let report: WeeklyReport = serde_json::from_str(&json_content)?;
            let title = format!("Expenses {} W{}", report.year, report.week);
            create_bar_chart(&report.by_category, &title)?
        }
        ReportType::Yearly => {
            let report: YearReport = serde_json::from_str(&json_content)?;
            let title = format!("Expenses {}", report.year);
            let data: Vec<(u32, u64)> =
                report.by_month.iter().map(|m| (m.month, m.total)).collect();
            create_year_chart(&data, &title)?
        }
    };

    let mut file = fs::File::create(&cli.output)?;
    file.write_all(&png_data)?;

    println!("Generated chart at {}", cli.output.display());

    Ok(())
}
