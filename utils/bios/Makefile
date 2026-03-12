all: bios.rom

bios.rom: bios.asm
	nasm -f bin -o $@ $<

clean:
	rm -f bios.rom

.PHONY: all clean
