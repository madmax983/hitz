1. **Optimize heap allocations in TUI loop (`crates/hitz-cli/src/main.rs`)**
    - The CLI application has a TUI mode (`handle_vm_top`) that constantly refreshes.
    - Inside this loop, it builds ratatui `Row`s. It creates new `String`s using `.clone()` or `format!` for every single cell, every frame.
    - `Row::new` accepts `IntoIterator` where the item implements `Into<Text>`.
    - We can pass `&d.name` or `d.name.as_str()` directly to `Row::new` without cloning the `String`.
    - Oh wait, `vec![d.name.clone(), format!(...)]` creates a `Vec<String>`.
    - We can use `vec![d.name.as_str().into(), format!(...).into()]` or similar, wait, `Row::new` takes an iterator. If we mix types (e.g. `&str` and `String`), we'll need to unify them into `std::borrow::Cow<'_, str>` or `Text`.
    - Ratatui's `Cell` type can be created from `&str` or `String`. We can just avoid `clone()` by using references or `Cow`. Or better yet, we can use `format!` for the numbers, and `d.name.as_str()` for the strings, and explicitly map them to `Cell`.
    - But there is an easier, 50-lines optimization:
    ```rust
    Row::new(vec![
        Cell::from(p.pid.to_string()),
        Cell::from(p.name.as_str()),
        Cell::from(format!("{:.1}%", p.cpu_pct)),
        Cell::from(format!("{} MB", p.rss_bytes / (1024 * 1024))),
    ])
    ```
    This completely eliminates the `.clone()` on strings (which might be long or allocate). Wait! We can also optimize `Row::new(vec![...])` to use array `[...]` to avoid `Vec` allocation entirely! `Row::new` takes `IntoIterator`. So `Row::new([Cell::from(...), ...])` works and avoids the `Vec` allocation per row.
    - The prompt says: "Memory allocations are the enemy.", "⚡ Replace `.clone()` with references `&T` or `Cow`."
