//! Terminal UI for stats dashboard
//!
//! This module implements the terminal user interface using ratatui,
//! displaying metrics in a table format similar to the `top` command.

use chrono::{DateTime, Utc};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

use crate::stats::parser::{AggregatedMetrics, GroupBy};

/// Application state for the stats dashboard
pub struct StatsApp {
    pub metrics: Vec<AggregatedMetrics>,
    pub last_update: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub group_by: GroupBy,
}

impl StatsApp {
    /// Create a new stats application
    pub fn new(group_by: GroupBy) -> Self {
        Self {
            metrics: Vec::new(),
            last_update: None,
            error_message: None,
            group_by,
        }
    }

    /// Handle keyboard input
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => return true, // Quit
            KeyCode::Char('1') => self.group_by = GroupBy::ApiKey,
            KeyCode::Char('2') => self.group_by = GroupBy::Provider,
            KeyCode::Char('3') => self.group_by = GroupBy::Model,
            KeyCode::Char('4') => self.group_by = GroupBy::All,
            _ => {}
        }
        false // Don't quit
    }

    /// Render the UI
    pub fn render(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(10),   // Table
                Constraint::Length(3), // Footer
            ])
            .split(f.area());

        self.render_header(f, chunks[0]);
        self.render_table(f, chunks[1]);
        self.render_footer(f, chunks[2]);
    }

    /// Render header with title and metadata
    fn render_header(&self, f: &mut Frame, area: Rect) {
        let group_name = match self.group_by {
            GroupBy::ApiKey => "API Key",
            GroupBy::Provider => "Provider",
            GroupBy::Model => "Model",
            GroupBy::All => "All Dimensions",
        };

        let last_update = self
            .last_update
            .map(|t| t.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "Never".to_string());

        let title = vec![
            Line::from(vec![
                Span::styled(
                    "LLM Gateway Stats",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" - Grouped by: "),
                Span::styled(group_name, Style::default().fg(Color::Yellow)),
                Span::raw("  |  Last update: "),
                Span::styled(last_update, Style::default().fg(Color::Green)),
            ]),
            Line::from(Span::styled(
                "Press 'q' to quit | 'r' to refresh | '1-4' to change grouping",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let paragraph = Paragraph::new(title).block(Block::default().borders(Borders::ALL));
        f.render_widget(paragraph, area);
    }

    /// Render metrics table
    fn render_table(&self, f: &mut Frame, area: Rect) {
        // Table headers
        let header_cells = [
            "Group",
            "Requests",
            "Input Tok",
            "Output Tok",
            "Errors",
            "Latency (ms)\nP50/P90/P99",
        ]
        .iter()
        .map(|h| {
            Cell::from(*h).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        });

        let header = Row::new(header_cells).height(2).bottom_margin(1);

        // Table rows
        let rows: Vec<Row> = if self.metrics.is_empty() {
            // Show "No data" message
            vec![Row::new(vec![Cell::from("No data available yet. Waiting for metrics...")])]
        } else {
            self.metrics
                .iter()
                .map(|m| {
                    let cells = vec![
                        Cell::from(m.group_key.clone()),
                        Cell::from(format_number(m.total_requests)),
                        Cell::from(format_number(m.total_input_tokens)),
                        Cell::from(format_number(m.total_output_tokens)),
                        Cell::from(if m.total_errors > 0 {
                            format!("{} ({:.1}%)", m.total_errors, m.error_rate)
                        } else {
                            "0".to_string()
                        }),
                        Cell::from(format_latency(
                            m.latency_p50,
                            m.latency_p90,
                            m.latency_p99,
                        )),
                    ];
                    Row::new(cells).height(1)
                })
                .collect()
        };

        let table = Table::new(
            rows,
            [
                Constraint::Percentage(20),
                Constraint::Percentage(13),
                Constraint::Percentage(13),
                Constraint::Percentage(13),
                Constraint::Percentage(13),
                Constraint::Percentage(28),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Metrics by Group"),
        )
        .column_spacing(1);

        f.render_widget(table, area);
    }

    /// Render footer with summary and errors
    fn render_footer(&self, f: &mut Frame, area: Rect) {
        let content = if let Some(error) = &self.error_message {
            // Show error message
            vec![
                Line::from(Span::styled(
                    format!("Error: {}", error),
                    Style::default().fg(Color::Red),
                )),
                Line::from(Span::styled(
                    "(retrying on next interval...)",
                    Style::default().fg(Color::Yellow),
                )),
            ]
        } else if !self.metrics.is_empty() {
            // Show summary statistics
            let total_requests: u64 = self.metrics.iter().map(|m| m.total_requests).sum();
            let total_input: u64 = self.metrics.iter().map(|m| m.total_input_tokens).sum();
            let total_output: u64 = self.metrics.iter().map(|m| m.total_output_tokens).sum();
            let total_errors: u64 = self.metrics.iter().map(|m| m.total_errors).sum();

            let error_rate = if total_requests > 0 {
                (total_errors as f64 / total_requests as f64) * 100.0
            } else {
                0.0
            };

            // Calculate overall average latency
            let avg_latency: Option<f64> = {
                let latencies: Vec<f64> = self
                    .metrics
                    .iter()
                    .filter_map(|m| m.avg_latency)
                    .collect();
                if !latencies.is_empty() {
                    Some(latencies.iter().sum::<f64>() / latencies.len() as f64)
                } else {
                    None
                }
            };

            vec![
                Line::from(vec![
                    Span::styled("Total: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!(
                        "{} requests | {} input tokens | {} output tokens",
                        format_number(total_requests),
                        format_number(total_input),
                        format_number(total_output)
                    )),
                ]),
                Line::from(vec![
                    Span::styled("Error Rate: ", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        format!("{:.2}%", error_rate),
                        if error_rate > 5.0 {
                            Style::default().fg(Color::Red)
                        } else if error_rate > 1.0 {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default().fg(Color::Green)
                        },
                    ),
                    Span::raw("  |  "),
                    Span::styled("Avg Latency: ", Style::default().fg(Color::Cyan)),
                    Span::raw(if let Some(lat) = avg_latency {
                        format!("{:.0}ms", lat * 1000.0)
                    } else {
                        "N/A".to_string()
                    }),
                ]),
            ]
        } else {
            vec![
                Line::from(Span::styled(
                    "Waiting for metrics data...",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(Span::styled(
                    "Make sure the gateway is running and has processed requests.",
                    Style::default().fg(Color::DarkGray),
                )),
            ]
        };

        let paragraph = Paragraph::new(content).block(Block::default().borders(Borders::ALL));
        f.render_widget(paragraph, area);
    }
}

/// Format number with thousand separators
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let len = s.len();

    for (i, c) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }

    result
}

/// Format latency percentiles (P50/P90/P99) in milliseconds
fn format_latency(p50: Option<f64>, p90: Option<f64>, p99: Option<f64>) -> String {
    match (p50, p90, p99) {
        (Some(p50), Some(p90), Some(p99)) => {
            format!(
                "{:.0}/{:.0}/{:.0}",
                p50 * 1000.0,
                p90 * 1000.0,
                p99 * 1000.0
            )
        }
        _ => "N/A".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(123), "123");
        assert_eq!(format_number(1234), "1,234");
        assert_eq!(format_number(1234567), "1,234,567");
        assert_eq!(format_number(1234567890), "1,234,567,890");
    }

    #[test]
    fn test_format_latency() {
        assert_eq!(
            format_latency(Some(0.1), Some(0.2), Some(0.3)),
            "100/200/300"
        );
        assert_eq!(
            format_latency(Some(0.05), Some(0.15), Some(0.25)),
            "50/150/250"
        );
        assert_eq!(format_latency(None, None, None), "N/A");
        assert_eq!(format_latency(Some(0.1), None, None), "N/A");
    }

    #[test]
    fn test_stats_app_creation() {
        let app = StatsApp::new(GroupBy::Provider);
        assert_eq!(app.group_by, GroupBy::Provider);
        assert!(app.metrics.is_empty());
        assert!(app.last_update.is_none());
        assert!(app.error_message.is_none());
    }
}
