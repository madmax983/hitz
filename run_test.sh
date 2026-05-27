#!/bin/bash
cat << 'INNER_EOF' >> crates/hitz-hal/src/traits.rs

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyVcpu {
        was_canceled: std::cell::Cell<bool>,
    }

    impl Vcpu for DummyVcpu {
        type CancelHandle = std::rc::Rc<std::cell::Cell<bool>>;

        fn run(&mut self) -> Result<VcpuExit, HalError> {
            Ok(VcpuExit::Halt)
        }

        fn cancel_handle(&self) -> Self::CancelHandle {
            std::rc::Rc::new(self.was_canceled.clone())
        }

        fn cancel_via(handle: &Self::CancelHandle) -> Result<(), HalError> {
            handle.set(true);
            Ok(())
        }

        fn get_regs(&self) -> Result<crate::StandardRegs, HalError> {
            unreachable!()
        }

        fn set_regs(&mut self, _: &crate::StandardRegs) -> Result<(), HalError> {
            unreachable!()
        }

        fn get_sregs(&self) -> Result<crate::SpecialRegs, HalError> {
            unreachable!()
        }

        fn set_sregs(&mut self, _: &crate::SpecialRegs) -> Result<(), HalError> {
            unreachable!()
        }

        fn inject_interrupt(&mut self, _: u8) -> Result<(), HalError> {
            unreachable!()
        }

        fn request_interrupt_window(&mut self) -> Result<(), HalError> {
            unreachable!()
        }
    }

    #[test]
    fn vcpu_cancel_default_impl() {
        let vcpu = DummyVcpu {
            was_canceled: std::cell::Cell::new(false),
        };
        vcpu.cancel().unwrap();
        assert!(vcpu.was_canceled.get());
    }
}
INNER_EOF
cargo test -p hitz-hal
