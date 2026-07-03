sed -i 's/let _ = mem.clone();/let _ = mem;/g' crates/hitz-hal/src/types.rs
sed -i 's/let _ = req.clone();/let _ = req;/g' crates/hitz-hal/src/types.rs
sed -i 's/let _ = pc.clone();/let _ = pc;/g' crates/hitz-hal/src/types.rs
sed -i 's/let _ = dt.clone();/let _ = dt;/g' crates/hitz-hal/src/types.rs
sed -i 's/let _ = sr.clone();/let _ = sr;/g' crates/hitz-hal/src/types.rs
sed -i 's/let _ = spr.clone();/let _ = spr;/g' crates/hitz-hal/src/types.rs
sed -i 's/let _ = sc.clone();/let _ = sc;/g' crates/hitz-hal/src/types.rs
sed -i 's/let _ = m.clone();/let _ = m;/g' crates/hitz-hal/src/types.rs
