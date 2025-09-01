.PHONY: all make_tmp install_tywaves_chiseltrace install_chiseltrace clean
TYWAVES_CHISEL_REPO = https://github.com/jarlb/tywaves-chisel.git
CARGO_BIN_DIR = ~/.cargo/bin/

all: install_tywaves_chiseltrace install_chiseltrace

make_tmp:
	mkdir -p tmp

clean:
	@rm -rf ./tmp

.ONESHELL:
install_tywaves_chiseltrace: make_tmp
	cd tmp
	git clone $(TYWAVES_CHISEL_REPO)
	cd tywaves-chisel
	make all

.ONESHELL:
install_chiseltrace:
	cargo tauri build --no-bundle
	cp ./target/release/chiseltrace $(CARGO_BIN_DIR)
