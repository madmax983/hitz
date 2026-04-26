import sys
with open('crates/hitz-api/src/health.rs', 'r') as f:
    content = f.read()

import re

new_content = re.sub(
    r'if self\.cpu\.total_pct > 90\.0 \{[\s\S]*?\} else if self\.cpu\.total_pct > 75\.0 \{[\s\S]*?\}',
    r'''match self.cpu.total_pct {
            pct if pct > 90.0 => {
                status = status.max(HealthStatus::Critical);
                reasons.push(format!("Critical CPU usage: {:.1}%", pct));
            }
            pct if pct > 75.0 => {
                status = status.max(HealthStatus::Warning);
                reasons.push(format!("High CPU usage: {:.1}%", pct));
            }
            _ => {}
        }''',
    content
)

with open('crates/hitz-api/src/health.rs', 'w') as f:
    f.write(new_content)
