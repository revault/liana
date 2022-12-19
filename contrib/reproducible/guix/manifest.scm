(specifications->manifest
  (append
    (list "rust"
          "rust:cargo"
          "coreutils"
          "patchelf"
          "gcc-toolchain@10.3.0"
          "musl")
          ;; Additional dependencies for building the GUI
          (let ((is_gui (getenv "IS_GUI")))
            (if
              (string=? is_gui "1")
              (list "pkg-config"
                    "eudev"
                    "fontconfig")
              '()))))
