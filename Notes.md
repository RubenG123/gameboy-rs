# NOTES
- Improve disassembler code. E.g. replace blank closures with func that inserts a delay instruction step?
- Implement LCD Control bit 7 behaviour
- Am I handling interupts in the correct order if more than 1 are valid to fire?
- really need start irq blocking implemented probs

- https://daid.github.io/GBEmulatorShootout/

- IE top 3 bits are writable, IF top 3 bits always 1
- https://discord.com/channels/465585922579103744/465586075830845475/854251537958305802

https://github.com/Gekkio/mooneye-gb/tree/master/tests/acceptance
https://gekkio.fi/files/mooneye-gb/latest/tests/acceptance/
https://github.com/Powerlated/TurtleTests

# Mooneye todo's
ei_timings: when ei happens, if IE & IF != 0 already then it takes 4 clock cycles before the interrupt takes place?

# PPU
- https://gbdev.io/pandocs/#pixel-fifo
- https://hacktixme.ga/GBEDG/ppu/#:~:text=The%20PPU%20(which%20stands%20for,beats%20the%20CPU%20by%20far
- https://www.reddit.com/r/EmuDev/comments/aihkvs/gb_some_interupt_questions/

# PPU Advanced
- https://github.com/mattcurrie/mealybug-tearoom-tests
- https://discord.com/channels/465585922579103744/465586075830845475/849370511338635334
- STAT IRQs and Blocking
- https://github.com/mattcurrie/cgb-acid-hell
- https://www.reddit.com/r/EmuDev/comments/8uahbc/dmg_bgb_lcd_timings_and_cnt/

- https://www.reddit.com/r/EmuDev/comments/59pawp/gb_mode3_sprite_timing/
    

- Run hblank_ly_scx_timing-GS.s in bgb and step through and see what the diff is? why isn't 
    setup and wait running??
- https://discord.com/channels/465585922579103744/465586075830845475/854344221086449684 mid fetch sc shift (undo the fix I did)
- http://blog.kevtris.org/blogfiles/Nitty%20Gritty%20Gameboy%20VRAM%20Timing.txt

- https://github.com/pinobatch/numism/tree/main/gameboy/exercise to help with sprite timings?
    Other emus show mode 3 timing as 42 cycles, maybe my interupt firing is slightly wrong?
    try adjusting +/- 4 cycles

- frame blending?


# Sound
- https://nightshade256.github.io/2021/03/27/gb-sound-emulation.html
- https://www.reddit.com/r/EmuDev/comments/5gkwi5/gb_apu_sound_emulation/dat3zni/
- https://gbdev.gg8.se/wiki/articles/Gameboy_sound_hardware
- https://gbdev.gg8.se/wiki/articles/Sound_Controller
- https://gbdev.io/pandocs/#sound-controller
- https://www.reddit.com/r/EmuDev/comments/5gkwi5/gb_apu_sound_emulation/

- PCM: Pulse code modulation
- 3 params
    - nyquest(?) freq: half the sampling rate (usually 44.1 or 48 kHz). The
        highest freq the pcm will be able to "describe"
    - sample format: (8 bit, a-law, mew-law, 16/24 signed bit, 32 bit float) the format used to
        describe the value of the amplitude of the signal 
    - number of channels: usually 2. values are interleaved between the channels in the data stream.

- https://sound.stackexchange.com/questions/34816/what-is-a-noise-sweep
- https://github.com/simias/gb-rs/blob/master/src/ui/sdl2/audio.rs
- https://www.youtube.com/watch?v=72dI7dB3ZvQ
- https://github.com/Rust-SDL2/rust-sdl2/blob/master/examples/audio-squarewave.rs
- https://emudev.de/gameboy-emulator/bleeding-ears-time-to-add-audio/
- https://timdaub.github.io/2020/02/19/wasm-synth/
- https://news.ycombinator.com/item?id=27273706

- https://gist.github.com/drhelius/3652407
- https://stackoverflow.com/questions/15087668/how-to-convert-pcm-samples-in-byte-array-as-floating-point-numbers-in-the-range

- https://stackoverflow.com/questions/24449957/converting-a-8-bit-pcm-to-16-bit-pcm

- https://github.com/LIJI32/SameSuite/tree/master/apu

# Node FFI
- https://stackoverflow.com/questions/36604010/how-can-i-build-multiple-binaries-with-cargo
- https://www.reddit.com/r/rust/comments/jg1qm2/electron_rust_how_to_talk_to_each_other/
- napi-rs / neon (think napi is better)
- node-rs?
- OR compile to wasm rather than ffi?? https://blog.logrocket.com/supercharge-your-electron-apps-with-rust/

# Misc
- DMA bus conflicts (ram trashing thing?)
- Implement oam dma as a completely separate thing that ticks over time
- https://stackoverflow.com/questions/17514598/building-a-cross-platform-application-using-rust
- https://stackoverflow.com/questions/29763647/how-to-make-a-program-that-does-not-display-the-console-window

# Testing
- mealybug roms should have their screenshot taken when the ld d, d "breakpoint" is hit
