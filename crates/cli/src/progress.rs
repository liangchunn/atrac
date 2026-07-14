use std::io::{self, IsTerminal, Write};

/// The stable progress surface shared by the two codec façades.
pub trait ProgressSnapshot: Copy {
    fn phase_label(self) -> &'static str;
    fn completed_steps(self) -> u32;
    fn total_steps(self) -> u32;
    fn completed_output_frames(self) -> u32;
    fn total_output_frames(self) -> u32;
}

impl ProgressSnapshot for at3::EncodeProgress {
    fn phase_label(self) -> &'static str {
        match self.phase {
            at3::EncodePhase::Encoding => "Encoding",
            at3::EncodePhase::Flushing => "Flushing",
        }
    }

    fn completed_steps(self) -> u32 {
        self.completed_steps
    }

    fn total_steps(self) -> u32 {
        self.total_steps
    }

    fn completed_output_frames(self) -> u32 {
        self.completed_output_frames
    }

    fn total_output_frames(self) -> u32 {
        self.total_output_frames
    }
}

impl ProgressSnapshot for at3p::EncodeProgress {
    fn phase_label(self) -> &'static str {
        match self.phase {
            at3p::EncodePhase::Encoding => "Encoding",
            at3p::EncodePhase::Flushing => "Flushing",
        }
    }

    fn completed_steps(self) -> u32 {
        self.completed_steps
    }

    fn total_steps(self) -> u32 {
        self.total_steps
    }

    fn completed_output_frames(self) -> u32 {
        self.completed_output_frames
    }

    fn total_output_frames(self) -> u32 {
        self.total_output_frames
    }
}

/// Interactive renderer for exact codec progress. Redirected stderr receives
/// no redraws; library callers can consume every update directly.
pub struct CliProgress {
    enabled: bool,
    active_line: bool,
}

impl CliProgress {
    pub fn new() -> Self {
        Self {
            enabled: io::stderr().is_terminal(),
            active_line: false,
        }
    }

    pub fn update<P: ProgressSnapshot>(&mut self, progress: P) {
        if !self.enabled {
            return;
        }

        let total_steps = progress.total_steps();
        let percent = if total_steps == 0 {
            100.0
        } else {
            f64::from(progress.completed_steps()) * 100.0 / f64::from(total_steps)
        };
        let mut stderr = io::stderr().lock();
        let _ = write!(
            stderr,
            "\r{}: {percent:5.1}% ({}/{}) - {}/{} output frames",
            progress.phase_label(),
            progress.completed_steps(),
            total_steps,
            progress.completed_output_frames(),
            progress.total_output_frames(),
        );
        let _ = stderr.flush();
        self.active_line = true;
    }

    pub fn finish(&mut self) {
        if self.active_line {
            eprintln!();
            self.active_line = false;
        }
    }
}
