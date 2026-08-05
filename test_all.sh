for crate in hitz-hal hitz-vmm hitz-boot hitz-devices hitz-api hitz-guest-agent hitz-whp; do
    echo "Testing $crate"
    cargo test -p $crate
done
