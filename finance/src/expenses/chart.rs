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

    let path = "/tmp/expense_chart.png";
    {
        let root = BitMapBackend::new(path, (600, 400)).into_drawing_area();

        root.fill(&RGBColor(20, 20, 30))?;

        let max_val = data.iter().map(|c| c.total).max().unwrap_or(1);

        let mut chart = ChartBuilder::on(&root)
            .caption(title, ("sans-serif", 20, FontStyle::Bold).into_font())
            .margin(20)
            .x_label_area_size(40)
            .y_label_area_size(60)
            .build_cartesian_2d(
                0i32..data.len() as i32,
                0i32..max_val as i32,
            )?;

        chart
            .configure_mesh()
            .x_labels(data.len())
            .x_label_formatter(&|x| {
                if *x >= 0 && *x < data.len() as i32 {
                    let name = &data[*x as usize].category_name;
                    if name.len() > 10 {
                        format!("{}...", &name[..8])
                    } else {
                        name.clone()
                    }
                } else {
                    String::new()
                }
            })
            .draw()?;

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

        chart.draw_series(data.iter().enumerate().map(|(i, item)| {
            let x = i as i32;
            let color = colors[i % colors.len()];

            Rectangle::new([(x, 0), (x + 1, item.total as i32)], color.filled())
        }))?;
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

    let path = "/tmp/year_chart.png";
    {
        let root = BitMapBackend::new(path, (800, 400)).into_drawing_area();

        root.fill(&RGBColor(20, 20, 30))?;

        let max_val = data.iter().map(|(_, v)| *v).max().unwrap_or(1);

        let mut chart = ChartBuilder::on(&root)
            .caption(title, ("sans-serif", 20, FontStyle::Bold).into_font())
            .margin(20)
            .x_label_area_size(40)
            .y_label_area_size(60)
            .build_cartesian_2d(0i32..12i32, 0i32..max_val as i32)?;

        chart
            .configure_mesh()
            .x_labels(12)
            .x_label_formatter(&|x| {
                let months = [
                    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug",
                    "Sep", "Oct", "Nov", "Dec",
                ];
                if *x >= 0 && *x < 12 {
                    months[*x as usize].to_string()
                } else {
                    String::new()
                }
            })
            .draw()?;

        chart.draw_series(data.iter().enumerate().map(|(i, (_, value))| {
            let x = i as i32;
            Rectangle::new(
                [(x, 0), (x + 1, *value as i32)],
                RGBColor(78, 205, 196).filled(),
            )
        }))?;
    }

    let mut buffer = Vec::new();
    fs::File::open(path)?.read_to_end(&mut buffer)?;
    let _ = fs::remove_file(path);

    Ok(buffer)
}
