//! Ratatui-based terminal rendering and the input/event loop.

use std::io::{self, Stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::{Frame, Terminal};

use crate::types::RobotKind;
use crate::world::WorldState;

type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Set up the terminal, run the draw loop, and guarantee restoration on exit.
pub fn event_loop(
    world: Arc<RwLock<WorldState>>,
    running: Arc<AtomicBool>,
    tick: Duration,
) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = draw_loop(&mut terminal, &world, &running, tick);

    // Always restore the terminal, even if the loop errored.
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, Show)?;
    terminal.show_cursor()?;
    result
}

fn draw_loop(
    terminal: &mut Tui,
    world: &Arc<RwLock<WorldState>>,
    running: &Arc<AtomicBool>,
    tick: Duration,
) -> io::Result<()> {
    while running.load(Ordering::Relaxed) {
        terminal.draw(|f| draw(f, world))?;

        // Any key press exits. `poll` doubles as the frame pacing timer.
        if event::poll(tick)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    running.store(false, Ordering::Relaxed);
                    break;
                }
            }
        }
    }
    Ok(())
}

fn draw(f: &mut Frame, world: &Arc<RwLock<WorldState>>) {
    let w = world.read().unwrap();

    let chunks = Layout::horizontal([
        Constraint::Length(w.map.width as u16 + 2),
        Constraint::Min(30),
    ])
    .split(f.area());

    render_map(f, chunks[0], &w);
    render_sidebar(f, chunks[1], &w);
}

/// Build the coloured map grid. Cell precedence: robot > base > resource > obstacle.
fn render_map(f: &mut Frame, area: ratatui::layout::Rect, w: &WorldState) {
    // Index robots by position for O(1) lookup (collectors drawn over scouts).
    let mut robots = std::collections::HashMap::new();
    for r in &w.robots {
        robots
            .entry(r.pos)
            .and_modify(|existing: &mut &crate::world::RobotView| {
                if r.kind == RobotKind::Collector {
                    *existing = r;
                }
            })
            .or_insert(r);
    }

    let mut lines: Vec<Line> = Vec::with_capacity(w.map.height as usize);
    for y in 0..w.map.height {
        let mut spans: Vec<Span> = Vec::with_capacity(w.map.width as usize);
        for x in 0..w.map.width {
            let p = crate::types::Position::new(x, y);
            let (ch, color) = if let Some(r) = robots.get(&p) {
                match r.kind {
                    RobotKind::Scout => ('x', Color::Red),
                    RobotKind::Collector => ('o', Color::Magenta),
                }
            } else if p == w.map.base {
                ('#', Color::LightGreen)
            } else if let Some(res) = w.map.resources.get(&p) {
                match res.kind {
                    crate::types::ResourceKind::Energy => ('E', Color::Green),
                    crate::types::ResourceKind::Crystal => ('C', Color::LightMagenta),
                }
            } else if w.map.is_obstacle_tile(p) {
                ('O', Color::LightCyan)
            } else {
                (' ', Color::Reset)
            };
            spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
        }
        lines.push(Line::from(spans));
    }

    let title = format!(" Resource Collection Simulation (seed {}) ", w.map.seed);
    let block = Block::bordered().title(title);
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_sidebar(f: &mut Frame, area: ratatui::layout::Rect, w: &WorldState) {
    let parts = Layout::vertical([Constraint::Min(14), Constraint::Min(6)]).split(area);

    let scouts = w
        .robots
        .iter()
        .filter(|r| r.kind == RobotKind::Scout)
        .count();
    let collectors = w
        .robots
        .iter()
        .filter(|r| r.kind == RobotKind::Collector)
        .count();

    let bold = Style::default().add_modifier(Modifier::BOLD);
    let stats = vec![
        Line::from(Span::styled("COLLECTED", bold)),
        Line::from(vec![
            Span::styled("  Energy:  ", Style::default()),
            Span::styled(
                w.total_energy.to_string(),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Crystal: ", Style::default()),
            Span::styled(
                w.total_crystals.to_string(),
                Style::default().fg(Color::LightMagenta),
            ),
        ]),
        Line::from(format!("  Deliveries: {}", w.deliveries)),
        Line::from(""),
        Line::from(Span::styled("MAP", bold)),
        Line::from(format!("  Deposits known: {}", w.discovered_resources)),
        Line::from(format!("  Deposits left:  {}", w.map.resources.len())),
        Line::from(format!("  Units left:     {}", w.map.remaining_units())),
        Line::from(""),
        Line::from(Span::styled("ROBOTS", bold)),
        Line::from(vec![
            Span::styled("  x ", Style::default().fg(Color::Red)),
            Span::raw(format!("Scouts:     {}", scouts)),
        ]),
        Line::from(vec![
            Span::styled("  o ", Style::default().fg(Color::Magenta)),
            Span::raw(format!("Collectors: {}", collectors)),
        ]),
        Line::from(""),
        Line::from(Span::styled("LEGEND", bold)),
        Line::from(vec![
            Span::styled("  O ", Style::default().fg(Color::LightCyan)),
            Span::raw("Obstacle   "),
            Span::styled("# ", Style::default().fg(Color::LightGreen)),
            Span::raw("Base"),
        ]),
        Line::from(vec![
            Span::styled("  E ", Style::default().fg(Color::Green)),
            Span::raw("Energy     "),
            Span::styled("C ", Style::default().fg(Color::LightMagenta)),
            Span::raw("Crystal"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Press any key to quit.",
            Style::default().add_modifier(Modifier::DIM),
        )),
    ];
    f.render_widget(
        Paragraph::new(stats)
            .block(Block::bordered().title(" Status "))
            .wrap(Wrap { trim: true }),
        parts[0],
    );

    let log_lines: Vec<Line> = w.log.iter().map(|l| Line::from(l.as_str())).collect();
    f.render_widget(
        Paragraph::new(log_lines)
            .block(Block::bordered().title(" Event Log "))
            .wrap(Wrap { trim: true }),
        parts[1],
    );
}
