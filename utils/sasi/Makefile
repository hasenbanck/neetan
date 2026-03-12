all: sasi.rom

sasi.rom: sasi.asm
	nasm -f bin -o $@ $<

clean:
	rm -f sasi.rom

.PHONY: all clean
