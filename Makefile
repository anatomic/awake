VERSION ?= $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
APP_NAME := Awake
BUNDLE := $(APP_NAME).app
BUILD_DIR := target
UNIVERSAL_DIR := $(BUILD_DIR)/universal/release

.PHONY: all clean build-arm64 build-x86_64 universal bundle package sign

all: package

clean:
	cargo clean
	rm -rf $(BUNDLE) $(APP_NAME).zip

build-arm64:
	cargo build --release --target aarch64-apple-darwin

build-x86_64:
	cargo build --release --target x86_64-apple-darwin

universal: build-arm64 build-x86_64
	mkdir -p $(UNIVERSAL_DIR)
	lipo -create \
		$(BUILD_DIR)/aarch64-apple-darwin/release/awake \
		$(BUILD_DIR)/x86_64-apple-darwin/release/awake \
		-output $(UNIVERSAL_DIR)/awake

bundle: universal
	mkdir -p $(BUNDLE)/Contents/MacOS
	mkdir -p $(BUNDLE)/Contents/Resources
	cp $(UNIVERSAL_DIR)/awake $(BUNDLE)/Contents/MacOS/awake
	sed 's/{{VERSION}}/$(VERSION)/g' scripts/Info.plist > $(BUNDLE)/Contents/Info.plist

sign: bundle
	./scripts/sign.sh $(BUNDLE)

package: bundle
	rm -f $(APP_NAME).zip
	ditto -c -k --keepParent $(BUNDLE) $(APP_NAME).zip
	@echo "Built $(APP_NAME).zip (version $(VERSION))"
