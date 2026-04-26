import sys

with open('crates/hitz-vmm/src/run_loop.rs', 'r') as f:
    content = f.read()

# Let's fix the match on Exit
search = '''
    match exit {
        VcpuExit::IoPort(io) => {
            record_exit(exit_counter, "IoPort");
            let mut devs = devices.lock().expect("device lock poisoned");
            handle_io_port(vcpu, &mut devs.serial, &io)?;
            Ok(None)
        }
        VcpuExit::Halt => {
            record_exit(exit_counter, "Halt");
            Ok(Some(ExitReason::Halt))
        }
        VcpuExit::Shutdown => {
            record_exit(exit_counter, "Shutdown");
            Ok(Some(ExitReason::Shutdown))
        }
        VcpuExit::Mmio(mmio) => {
            record_exit(exit_counter, "Mmio");
            handle_mmio(vcpu, devices, mem, &mmio, pending_irq)?;
            Ok(None)
        }
        VcpuExit::InterruptWindow => {
            record_exit(exit_counter, "InterruptWindow");
            // Guest is now interruptible. WHP auto-clears the
            // deliverability notification after this exit fires.
            if let Some(vector) = pending_irq.take() {
                vcpu.inject_interrupt(vector)?;
                tracing::debug!(vector, "deferred interrupt injected via interrupt window");
            }
            Ok(None)
        }
        VcpuExit::Canceled => {
            record_exit(exit_counter, "Canceled");
            // vCPU run was canceled (e.g. by another thread).
            // If we have a pending IRQ, re-request the interrupt window
            // so we get notified once the guest becomes interruptible.
            if pending_irq.is_some() {
                vcpu.request_interrupt_window()?;
            }
            Ok(None)
        }
        VcpuExit::Unknown(code) => {
            record_exit(exit_counter, "Unexpected");
            Ok(Some(ExitReason::Unexpected(format!(
                "unknown vCPU exit reason: {code:#x}"
            ))))
        }
    }
'''

replace = '''
    match exit {
        VcpuExit::IoPort(io) => {
            record_exit(exit_counter, "IoPort");
            let mut devs = devices.lock().expect("device lock poisoned");
            handle_io_port(vcpu, &mut devs.serial, &io).map(|_| None)
        }
        VcpuExit::Halt => {
            record_exit(exit_counter, "Halt");
            Ok(Some(ExitReason::Halt))
        }
        VcpuExit::Shutdown => {
            record_exit(exit_counter, "Shutdown");
            Ok(Some(ExitReason::Shutdown))
        }
        VcpuExit::Mmio(mmio) => {
            record_exit(exit_counter, "Mmio");
            handle_mmio(vcpu, devices, mem, &mmio, pending_irq).map(|_| None)
        }
        VcpuExit::InterruptWindow => {
            record_exit(exit_counter, "InterruptWindow");
            if let Some(vector) = pending_irq.take() {
                vcpu.inject_interrupt(vector)?;
                tracing::debug!(vector, "deferred interrupt injected via interrupt window");
            }
            Ok(None)
        }
        VcpuExit::Canceled => {
            record_exit(exit_counter, "Canceled");
            if pending_irq.is_some() {
                vcpu.request_interrupt_window()?;
            }
            Ok(None)
        }
        VcpuExit::Unknown(code) => {
            record_exit(exit_counter, "Unexpected");
            Ok(Some(ExitReason::Unexpected(format!("unknown vCPU exit reason: {code:#x}"))))
        }
    }
'''

new_content = content.replace(search.strip(), replace.strip())

with open('crates/hitz-vmm/src/run_loop.rs', 'w') as f:
    f.write(new_content)
