//! TUI rendering with ratatui.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{participant::ParticipantView, status::SidecarStatus};

/// Minimum width for each participant panel.
/// Set to 26 to get 3 columns on an 80-char terminal (2 rows of 3 for 6 participants).
const MIN_PANEL_WIDTH: u16 = 26;

/// Renders the application state to the terminal.
pub fn render(frame: &mut Frame<'_>, views: &[ParticipantView], status: &SidecarStatus) {
    let area = frame.area();

    // Split into header and main content
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Render header
    render_header(frame, main_chunks[0], status);

    // Render participants in a flexible grid
    render_participants_grid(frame, main_chunks[1], views, status);
}

/// Renders participants in a grid layout based on available width.
fn render_participants_grid(
    frame: &mut Frame<'_>,
    area: Rect,
    views: &[ParticipantView],
    status: &SidecarStatus,
) {
    if views.is_empty() {
        return;
    }

    // Calculate how many participants fit per row
    let cols_per_row = (area.width / MIN_PANEL_WIDTH).max(1) as usize;
    let cols_per_row = cols_per_row.min(views.len());

    // Calculate number of rows needed
    let num_rows = views.len().div_ceil(cols_per_row);

    // Create row constraints
    let row_constraints: Vec<Constraint> =
        (0..num_rows).map(|_| Constraint::Ratio(1, num_rows as u32)).collect();

    let row_chunks =
        Layout::default().direction(Direction::Vertical).constraints(row_constraints).split(area);

    // Render each row
    for (row_idx, row_area) in row_chunks.iter().enumerate() {
        let start_idx = row_idx * cols_per_row;
        let end_idx = (start_idx + cols_per_row).min(views.len());
        let row_views = &views[start_idx..end_idx];

        // Create column constraints for this row
        let col_constraints: Vec<Constraint> =
            row_views.iter().map(|_| Constraint::Ratio(1, row_views.len() as u32)).collect();

        let col_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(*row_area);

        for (col_idx, view) in row_views.iter().enumerate() {
            render_participant(frame, col_chunks[col_idx], view, status);
        }
    }
}

/// Renders the header showing sidecar activity.
fn render_header(frame: &mut Frame<'_>, area: Rect, status: &SidecarStatus) {
    let block = Block::default()
        .title(" Arturo Demo - Sequencer Consensus ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let status_line = Line::from(vec![
        Span::styled("Chain: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            &status.action,
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("Epoch: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            status.epoch.to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("Blocks: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            status.certified_blocks.to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("[q] quit", Style::default().fg(Color::DarkGray)),
    ]);

    let paragraph = Paragraph::new(status_line);
    frame.render_widget(paragraph, inner_area);
}

/// Renders a single participant panel.
fn render_participant(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &ParticipantView,
    status: &SidecarStatus,
) {
    let (border_color, role_style) = if view.is_leader {
        (Color::Green, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
    } else {
        (Color::Gray, Style::default().fg(Color::Gray))
    };

    let role_text = if view.is_leader { "LEADER" } else { "Validator" };

    let title = format!(" P{} ", view.id);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    // Create content lines using shared status for consistency
    let lines = vec![
        Line::from(vec![Span::styled(role_text, role_style)]),
        Line::from(vec![
            Span::styled("Epoch: ", Style::default().fg(Color::DarkGray)),
            Span::styled(status.epoch.to_string(), Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Blocks: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                status.certified_blocks.to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner_area);
}
