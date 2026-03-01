use crate::expenses::report::CategoryTotal;
use plotters::prelude::*;
use std::fs;
use std::io::Read;

pub fn create_bar_chart(
    data: &[CategoryTotal],
    title: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let total: u64 = data.iter().map(|c| c.total).sum();
    if total == 0 || data.is_empty() {
        return Ok(Vec::new());
    }

    let path = "/tmp/expense_chart.svg";
    {
        let root = SVGBackend::new(path, (600, 400)).into_drawing_area();

        root.fill(&RGBColor(20, 20, 30))?;

        let max_val = data.iter().map(|c| c.total).max().unwrap_or(1);

        let mut chart = ChartBuilder::on(&root)
            .caption(title, ("sans-serif", 20, FontStyle::Bold).into_font())
            .margin(10)
            .x_label_area_size(30)
            .y_label_area_size(60)
            .build_cartesian_2d(
                0i32..data.len() as i32,
                0i32..max_val as i32,
            )?;

        chart.configure_mesh().draw()?;

        let colors = [
            RGBColor(255, 107, 107),
            RGBColor(78, 205, 196),
            RGBColor(255, 230, 109),
            RGBColor(26, 83, 92),
            RGBColor(255, 159, 67),
            RGBColor(84, 160, 255),
            RGBColor(95, 39, 205),
            RGBColor(29, 209, 161),
        ];

        for (i, item) in data.iter().enumerate() {
            let x = i as i32;
            let color = colors[i % colors.len()];

            root.draw(&Rectangle::new(
                [(x, 0), (x + 1, item.total as i32)],
                color.filled(),
            ))?;
        }
    }

    let mut buffer = Vec::new();
    fs::File::open(path)?.read_to_end(&mut buffer)?;
    let _ = fs::remove_file(path);

    Ok(buffer)
}

pub fn create_year_chart(
    data: &[(u32, u64)],
    title: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let total: u64 = data.iter().map(|(_, v)| v).sum();
    if total == 0 || data.is_empty() {
        return Ok(Vec::new());
    }

    let path = "/tmp/year_chart.svg";
    {
        let root = SVGBackend::new(path, (800, 400)).into_drawing_area();

        root.fill(&RGBColor(20, 20, 30))?;

        let max_val = data.iter().map(|(_, v)| *v).max().unwrap_or(1);

        let max_val_i32 = max_val as i32;

        let mut chart = ChartBuilder::on(&root)
            .caption(title, ("sans-serif", 20, FontStyle::Bold).into_font())
            .margin(10)
            .x_label_area_size(30)
            .y_label_area_size(60)
            .build_cartesian_2d(0i32..12i32, 0i32..max_val_i32)?;

        chart.configure_mesh().draw()?;

        for (i, (_, value)) in data.iter().enumerate() {
            let x = i as i32;

            root.draw(&Rectangle::new(
                [(x, 0), (x + 1, *value as i32)],
                RGBColor(78, 205, 196).filled(),
            ))?;
        }
    }

    let mut buffer = Vec::new();
    fs::File::open(path)?.read_to_end(&mut buffer)?;
    let _ = fs::remove_file(path);

    Ok(buffer)
}
