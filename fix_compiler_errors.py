import re

with open("crates/hitz-cli/src/main.rs", "r") as f:
    content = f.read()

content = content.replace("make_bar(snap.cpu.total_pct, 15)", "make_bar(snap.cpu.total_pct as f64, 15)")
content = content.replace("color_for_pct(snap.cpu.total_pct)", "color_for_pct(snap.cpu.total_pct as f64)")
content = content.replace("color_for_pct(proc.cpu_pct)", "color_for_pct(proc.cpu_pct as f64)")

# For the `Paragraph::new(err.as_str())`, the `err` is a `&String` because of `if let Some(ref err) = last_err`. Let's dereference it to string slice.
content = content.replace("Paragraph::new(err.as_str())", "Paragraph::new(err.as_str())")
# Actually, the error `str_as_str` is because the `err` is somehow resolved as `str` type, but it's a String. Let's replace `if let Some(ref err) = last_err {` with `if let Some(err) = last_err.as_deref() {` and `err.as_str()` with `err`
content = content.replace("if let Some(ref err) = last_err {", "if let Some(err) = last_err.as_deref() {")
content = content.replace("let err_p = Paragraph::new(err.as_str())", "let err_p = Paragraph::new(err)")


with open("crates/hitz-cli/src/main.rs", "w") as f:
    f.write(content)
