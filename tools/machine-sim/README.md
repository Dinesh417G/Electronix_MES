# tools/machine-sim — reserved (built at M2, extended at M4)

Scripted virtual machines that emit MQTT/HTTP production signals for end-to-end
testing (§6, §13). Gains a **virtual dnc-daemon mode** at M4 so the DNC
transfer/edit-back flow is testable against a simulated daemon, never real CNC
hardware (§13). Directory reserved until M2.
