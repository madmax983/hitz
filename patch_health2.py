import sys
with open('crates/hitz-api/src/health.rs', 'r') as f:
    content = f.read()

import re

search = '''
        // Memory evaluation
        if self.memory.total_bytes > 0 {
            let memory_pct =
                (self.memory.used_bytes as f64 / self.memory.total_bytes as f64) * 100.0;
            if memory_pct > 90.0 {
                status = status.max(HealthStatus::Critical);
                reasons.push(format!("Critical memory usage: {memory_pct:.1}%"));
            } else if memory_pct > 75.0 {
                status = status.max(HealthStatus::Warning);
                reasons.push(format!("High memory usage: {memory_pct:.1}%"));
            }
        }
'''

replace = '''
        // Memory evaluation
        if self.memory.total_bytes > 0 {
            let memory_pct =
                (self.memory.used_bytes as f64 / self.memory.total_bytes as f64) * 100.0;
            match memory_pct {
                pct if pct > 90.0 => {
                    status = status.max(HealthStatus::Critical);
                    reasons.push(format!("Critical memory usage: {pct:.1}%"));
                }
                pct if pct > 75.0 => {
                    status = status.max(HealthStatus::Warning);
                    reasons.push(format!("High memory usage: {pct:.1}%"));
                }
                _ => {}
            }
        }
'''

new_content = content.replace(search.strip(), replace.strip())

search2 = '''
        // Swap evaluation
        if self.memory.swap_total > 0 {
            #[allow(clippy::cast_precision_loss)]
            let swap_pct = (self.memory.swap_used as f64 / self.memory.swap_total as f64) * 100.0;
            if swap_pct > 50.0 {
                status = status.max(HealthStatus::Critical);
                reasons.push(format!("Critical swap usage: {swap_pct:.1}%"));
            } else if swap_pct > 20.0 {
                status = status.max(HealthStatus::Warning);
                reasons.push(format!("High swap usage: {swap_pct:.1}%"));
            }
        }
'''

replace2 = '''
        // Swap evaluation
        if self.memory.swap_total > 0 {
            #[allow(clippy::cast_precision_loss)]
            let swap_pct = (self.memory.swap_used as f64 / self.memory.swap_total as f64) * 100.0;
            match swap_pct {
                pct if pct > 50.0 => {
                    status = status.max(HealthStatus::Critical);
                    reasons.push(format!("Critical swap usage: {pct:.1}%"));
                }
                pct if pct > 20.0 => {
                    status = status.max(HealthStatus::Warning);
                    reasons.push(format!("High swap usage: {pct:.1}%"));
                }
                _ => {}
            }
        }
'''

new_content = new_content.replace(search2.strip(), replace2.strip())

search3 = '''
        // Network errors evaluation
        let total_net_errors: u64 = self
            .networks
            .iter()
            .map(|net| net.rx_errors + net.tx_errors)
            .sum();

        if total_net_errors > 100 {
            status = status.max(HealthStatus::Critical);
            reasons.push(format!("Critical network errors: {total_net_errors}"));
        } else if total_net_errors > 10 {
            status = status.max(HealthStatus::Warning);
            reasons.push(format!("Elevated network errors: {total_net_errors}"));
        }
'''

replace3 = '''
        // Network errors evaluation
        let total_net_errors: u64 = self
            .networks
            .iter()
            .map(|net| net.rx_errors + net.tx_errors)
            .sum();

        match total_net_errors {
            errs if errs > 100 => {
                status = status.max(HealthStatus::Critical);
                reasons.push(format!("Critical network errors: {errs}"));
            }
            errs if errs > 10 => {
                status = status.max(HealthStatus::Warning);
                reasons.push(format!("Elevated network errors: {errs}"));
            }
            _ => {}
        }
'''

new_content = new_content.replace(search3.strip(), replace3.strip())

with open('crates/hitz-api/src/health.rs', 'w') as f:
    f.write(new_content)
