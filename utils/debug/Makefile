all: debug_grcg.rom debug_text.rom debug_egc.rom debug_mouse.rom debug_crtc.rom debug_fpu.rom

debug_grcg.rom: debug_grcg.asm
	nasm -f bin -o $@ $<

debug_text.rom: debug_text.asm
	nasm -f bin -o $@ $<

debug_egc.rom: debug_egc.asm
	nasm -f bin -o $@ $<

debug_mouse.rom: debug_mouse.asm
	nasm -f bin -o $@ $<

debug_crtc.rom: debug_crtc.asm
	nasm -f bin -o $@ $<

debug_fpu.rom: debug_fpu.asm
	nasm -f bin -o $@ $<

clean:
	rm -f debug_grcg.rom debug_text.rom debug_egc.rom debug_mouse.rom debug_crtc.rom debug_fpu.rom

.PHONY: all clean
