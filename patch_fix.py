import re
with open("crates/hitz-api/src/prometheus.rs", "r") as f:
    content = f.read()

# Add doc comment for the Bolt Optimization
doc_comment = """
        // ⚡ Bolt Optimization: Eliminated `format!` heap allocations for Prometheus labels.
        // We use `format_args!` instead, which passes the formatting arguments
        // directly to the underlying `writeln!` without creating intermediate `String`s.
"""
content = content.replace("        // --- CPU Metrics ---", doc_comment + "        // --- CPU Metrics ---")

# Replace `format!` inline for cpu
content = content.replace('format!("core=\\"{}\\"", i)', 'format_args!("core=\\"{}\\"", i)')

# Replace disk metrics let block
disk_regex = r'let labels = format!\("device=\\"\{\}\\"", disk\.name\);\n\s+metric!\(\n\s+"hitz_disk_reads_total",\n\s+"counter",\n\s+"Total completed read operations",\n\s+labels,\n\s+disk\.reads_total\n\s+\);\n\s+metric!\(\n\s+"hitz_disk_writes_total",\n\s+"counter",\n\s+"Total completed write operations",\n\s+labels,\n\s+disk\.writes_total\n\s+\);\n\s+metric!\(\n\s+"hitz_disk_read_bytes_total",\n\s+"counter",\n\s+"Total bytes read",\n\s+labels,\n\s+disk\.read_bytes\n\s+\);\n\s+metric!\(\n\s+"hitz_disk_write_bytes_total",\n\s+"counter",\n\s+"Total bytes written",\n\s+labels,\n\s+disk\.write_bytes\n\s+\);'
disk_repl = r'''metric!(
                "hitz_disk_reads_total",
                "counter",
                "Total completed read operations",
                format_args!("device=\"{}\"", disk.name),
                disk.reads_total
            );
            metric!(
                "hitz_disk_writes_total",
                "counter",
                "Total completed write operations",
                format_args!("device=\"{}\"", disk.name),
                disk.writes_total
            );
            metric!(
                "hitz_disk_read_bytes_total",
                "counter",
                "Total bytes read",
                format_args!("device=\"{}\"", disk.name),
                disk.read_bytes
            );
            metric!(
                "hitz_disk_write_bytes_total",
                "counter",
                "Total bytes written",
                format_args!("device=\"{}\"", disk.name),
                disk.write_bytes
            );'''
content = re.sub(disk_regex, disk_repl, content)

# Replace network metrics let block
net_regex = r'let labels = format!\("interface=\\"\{\}\\"", net\.interface\);\n\s+metric!\(\n\s+"hitz_net_rx_bytes_total",\n\s+"counter",\n\s+"Total bytes received",\n\s+labels,\n\s+net\.rx_bytes\n\s+\);\n\s+metric!\(\n\s+"hitz_net_tx_bytes_total",\n\s+"counter",\n\s+"Total bytes transmitted",\n\s+labels,\n\s+net\.tx_bytes\n\s+\);\n\s+metric!\(\n\s+"hitz_net_rx_packets_total",\n\s+"counter",\n\s+"Total packets received",\n\s+labels,\n\s+net\.rx_packets\n\s+\);\n\s+metric!\(\n\s+"hitz_net_tx_packets_total",\n\s+"counter",\n\s+"Total packets transmitted",\n\s+labels,\n\s+net\.tx_packets\n\s+\);\n\s+metric!\(\n\s+"hitz_net_rx_errors_total",\n\s+"counter",\n\s+"Total receive errors",\n\s+labels,\n\s+net\.rx_errors\n\s+\);\n\s+metric!\(\n\s+"hitz_net_tx_errors_total",\n\s+"counter",\n\s+"Total transmit errors",\n\s+labels,\n\s+net\.tx_errors\n\s+\);'
net_repl = r'''metric!(
                "hitz_net_rx_bytes_total",
                "counter",
                "Total bytes received",
                format_args!("interface=\"{}\"", net.interface),
                net.rx_bytes
            );
            metric!(
                "hitz_net_tx_bytes_total",
                "counter",
                "Total bytes transmitted",
                format_args!("interface=\"{}\"", net.interface),
                net.tx_bytes
            );
            metric!(
                "hitz_net_rx_packets_total",
                "counter",
                "Total packets received",
                format_args!("interface=\"{}\"", net.interface),
                net.rx_packets
            );
            metric!(
                "hitz_net_tx_packets_total",
                "counter",
                "Total packets transmitted",
                format_args!("interface=\"{}\"", net.interface),
                net.tx_packets
            );
            metric!(
                "hitz_net_rx_errors_total",
                "counter",
                "Total receive errors",
                format_args!("interface=\"{}\"", net.interface),
                net.rx_errors
            );
            metric!(
                "hitz_net_tx_errors_total",
                "counter",
                "Total transmit errors",
                format_args!("interface=\"{}\"", net.interface),
                net.tx_errors
            );'''
content = re.sub(net_regex, net_repl, content)

# Replace process metrics let block
proc_regex = r'let labels = format!\("pid=\\"\{\}\\",name=\\"\{\}\\"", proc\.pid, proc\.name\);\n\s+metric!\(\n\s+"hitz_process_cpu_pct",\n\s+"gauge",\n\s+"CPU utilisation percentage for process",\n\s+labels,\n\s+proc\.cpu_pct\n\s+\);\n\s+metric!\(\n\s+"hitz_process_rss_bytes",\n\s+"gauge",\n\s+"Resident set size in bytes for process",\n\s+labels,\n\s+proc\.rss_bytes\n\s+\);'
proc_repl = r'''metric!(
                "hitz_process_cpu_pct",
                "gauge",
                "CPU utilisation percentage for process",
                format_args!("pid=\"{}\",name=\"{}\"", proc.pid, proc.name),
                proc.cpu_pct
            );
            metric!(
                "hitz_process_rss_bytes",
                "gauge",
                "Resident set size in bytes for process",
                format_args!("pid=\"{}\",name=\"{}\"", proc.pid, proc.name),
                proc.rss_bytes
            );'''
content = re.sub(proc_regex, proc_repl, content)

with open("crates/hitz-api/src/prometheus.rs", "w") as f:
    f.write(content)
