all: os.rom

os.rom: os.asm
	nasm -f bin -o $@ $<

clean:
	rm -f os.rom

.PHONY: all clean
