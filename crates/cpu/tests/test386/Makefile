# Regenerate test386.bin from the vendored upstream source under upstream/.
# Our configuration.asm (next to this Makefile) is picked up first via the
# include search order.
#
# Usage: run `make` from this directory.

UPSTREAM := upstream/src
NASM     := nasm

test386.bin: configuration.asm $(UPSTREAM)/test386.asm
	$(NASM) -i./ -i$(UPSTREAM)/ -f bin $(UPSTREAM)/test386.asm \
	        -w-all -l test386.lst -o test386.bin

clean:
	rm -f test386.bin test386.lst

.PHONY: clean
