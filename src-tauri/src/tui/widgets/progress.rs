use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Gauge, Widget},
};

#[allow(dead_code)]
pub struct DownloadBar<'a> {
    pub label: &'a str,
    pub percent: f64,
    pub downloaded: u64,
    pub total: u64,
    pub status: &'a str,
}

impl<'a> Widget for DownloadBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 {
            return;
        }

        // Line 1: label + status
        let status_style = match self.status {
            "downloading" => Style::default().fg(Color::Green),
            "retrying" => Style::default().fg(Color::Yellow),
            "paused" | "extracting" => Style::default().fg(Color::Yellow),
            "error" | "failed" => Style::default().fg(Color::Red),
            "complete" => Style::default().fg(Color::Green),
            _ => Style::default().fg(Color::White),
        };

        let label_line = Line::from(vec![
            Span::styled(self.label, Style::default().fg(Color::White)),
            Span::raw("  "),
            Span::styled(self.status, status_style),
        ]);
        label_line.render(
            Rect {
                x: area.x + 1,
                y: area.y,
                width: area.width.saturating_sub(1),
                height: 1,
            },
            buf,
        );

        // Line 2: progress bar + size
        let size_text = format_download_size(self.downloaded, self.total);
        let gauge_width = area.width.saturating_sub(size_text.len() as u16 + 3);

        if gauge_width > 4 {
            let gauge = Gauge::default()
                .gauge_style(Style::default().fg(Color::Green).bg(Color::Blue))
                .ratio(self.percent.clamp(0.0, 1.0))
                .label(format!("{:.0}%", self.percent * 100.0));
            gauge.render(
                Rect {
                    x: area.x + 1,
                    y: area.y + 1,
                    width: gauge_width,
                    height: 1,
                },
                buf,
            );
        }

        let size_span = Span::styled(size_text, Style::default().fg(Color::Blue));
        Line::from(size_span).render(
            Rect {
                x: area.x + gauge_width + 2,
                y: area.y + 1,
                width: area.width.saturating_sub(gauge_width + 2),
                height: 1,
            },
            buf,
        );
    }
}

#[allow(dead_code)]
fn format_download_size(downloaded: u64, total: u64) -> String {
    let dl_gb = downloaded as f64 / (1024.0 * 1024.0 * 1024.0);
    let total_gb = total as f64 / (1024.0 * 1024.0 * 1024.0);
    if total_gb >= 1.0 {
        format!("{:.1}/{:.1} GB", dl_gb, total_gb)
    } else {
        let dl_mb = downloaded as f64 / (1024.0 * 1024.0);
        let total_mb = total as f64 / (1024.0 * 1024.0);
        format!("{:.0}/{:.0} MB", dl_mb, total_mb)
    }
}
