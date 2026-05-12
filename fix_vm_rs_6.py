import re

with open("crates/hitz-vmm/src/vm.rs", "r") as f:
    code = f.read()

code = code.replace("let _ = handle.join();", "let _ = handle.join();") # oops, I replaced it before but it reverted?
code = code.replace("""    for handle in handles {
        let _ = handle.join();
    }""", """    for handle in handles {
        let _ = handle.join();
    }""") # Wait, since handles is now Vec<JoinHandle<()>>, we don't need ? on handle.
# Oh, my regex collect replacement might have not replaced it properly, or handles is Vec<Result<JoinHandle, VmError>>?
# No, collect::<Result<Vec<_>, VmError>>()? turns it into Vec<JoinHandle>.

# Wait! If it says `method not found in Result<JoinHandle<()>, VmError>`, it means `handles` is an iterator of `Result`s because I didn't successfully `.collect::<Result<Vec<_>, VmError>>()?`!
# Let's check `vm.rs` contents.
