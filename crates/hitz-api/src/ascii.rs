//! ASCII Chart Generator Module
//!
//! # Abstract
//! Generates a simple text-based horizontal bar chart representation of `CpuMetrics`.
//! This is useful for terminal logs and CLI tools to quickly visualize CPU load
//! across cores without needing external charting libraries.

use crate::CpuMetrics;
use std::fmt::Write;

/// Trait to export structures to ASCII charts.
pub trait AsciiChart {
    /// Returns the ASCII bar chart representation as a String.
    fn to_ascii_chart(&self) -> String;
}

impl AsciiChart for CpuMetrics {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn to_ascii_chart(&self) -> String {
        let mut chart = String::new();

        let blocks = "█".repeat((self.total_pct / 10.0) as usize);
        let total = self.total_pct;
        let _ = writeln!(chart, "Total: [{blocks:10}] {total:.1}%");

        for (i, &pct) in self.per_core.iter().enumerate() {
            let num_blocks = (pct / 10.0) as usize;
            let core_blocks = "█".repeat(num_blocks);
            let _ = writeln!(chart, "CPU [{i}]: [{core_blocks:10}] {pct:.1}%");
        }

        chart
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_ascii_chart() {
        let cpu = CpuMetrics {
            total_pct: 50.0,
            per_core: vec![25.0, 75.0, 100.0, 0.0],
            load_avg: [0.0, 0.0, 0.0],
        };

        let chart = cpu.to_ascii_chart();
        assert!(chart.contains("CPU [0]:"));
        assert!(chart.contains("███████   ] 75.0%")); // checking specific line format
        assert!(chart.contains("██████████] 100.0%"));
    }
}
