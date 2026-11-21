## 共用 Shell script 規範

### 核心原則

- 能直接可靠完成的流程保持直接；不加入無必要的 wrapper、預檢或 fallback（細則見「檢查與驗證」與「錯誤處理」）。
- 標示為 upstream-only 或要求 downstream 原樣同步的外部 template 禁止在下游修改；發現問題時應修正上游，再完整同步至下游。
- Bash script 需要 Bash 4.3 以上（共用 library 使用 nameref）；macOS 使用 Homebrew bash，不支援系統內建的 Bash 3.2。
- 已有共用 helper 的功能直接使用，不在腳本內重寫，例如 `require_root`、`confirm`、`install_file`、`detect_architecture`、`canonical_path`。
- 刪除目標含外部輸入或計算結果（參數、環境變數、檔案內容），且未經嚴格格式驗證時，使用 `safe_rm_under_root`；由 `readonly` 固定路徑，或由已驗證不含 `/` 的名稱組成的路徑，直接使用 `rm -rf --`。

### 結構與可讀性

- 可執行腳本通常放在 `scripts/<domain>/<platform>/`；共用 Bash library 放 `scripts/libs/`；封裝模板與 maintainer assets 放 `packaging/`。
- 專案自有的可執行 Bash script 依下列範本的順序與空行排列，並省略沒有內容的 section：

```bash
#!/usr/bin/env bash

# shellcheck shell=bash
# shellcheck disable=SC1091

set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
source "${SCRIPT_DIR}/libs/common.sh"

# Constants/Variables
readonly EXAMPLE_VALUE='example'

# Functions
helper() {
    :
}

# Run
main() {
    helper
}

main "$@"
```

- `SCRIPT_DIR` 與其後的 `source` 屬於載入 library 所需的 bootstrap，放在 section 外。
- ShellCheck directive 只加入實際需要的項目，例如使用動態 `source` 路徑時才停用 `SC1091`。
- `# Constants/Variables` 放 script 自身的全域常數與必要的可變狀態，常數以 `readonly` 標記；只在函式內使用的狀態優先宣告為 `local`，不放全域變數區。
- 含 command substitution 的 `readonly`、`local`、`export` 先賦值再單獨宣告（SC2155），避免掩蓋失敗狀態。
- `# Functions` 放共用或輔助函式。
- 依專案選擇主要 repo-local root，例如 `.build/`、`.cache/` 或 `.tool/`，集中放置 cache、下載與工具；除非確有必要，不散落多個 `.xxx/` 目錄。Cargo、APT、Snap 等 package manager 安裝的工具使用其標準位置。
- 最終交付產物放 `dist/`；不依賴執行者目前所在目錄，由腳本位置推導 repo root。
- 函式與 local 變數使用 `snake_case`；常數與對外環境變數使用 `UPPER_SNAKE_CASE`。
- 變數展開原則上使用雙引號；固定字串優先使用單引號。只有所有目標平台的該指令都支援時，才在路徑參數前使用 `--`；不得為形式一致而犧牲可攜性，例如 macOS BSD `chmod` 不使用 `--`。
- Bash 條件判斷使用 `[[ ]]`；命令參數以陣列組裝後以 `"${args[@]}"` 展開，不依賴字串分詞。
- 註解只解釋原因、限制或非直觀行為。

### 入口

- 所有作為獨立入口執行的腳本必須定義 `main()`；一般入口使用 `main() { ... }`，需要 local trap 或環境隔離時使用 subshell `main() ( ... )`。
- `# Run` 放在 `main` 定義正上方；`main` 之後保留一個空行再直接執行 `main "$@"`，中間不得插入其他內容。Self-extracting script 可在其後附加格式要求的 payload。
- 不支援 source 的腳本不加 `BASH_SOURCE` guard。
- 最終 foreground wrapper 優先在 `main` 內使用 `exec`；若 parent 必須保留且可能直接收到 signal，或管理 background、supervisor、daemon child，必須正確處理 TERM、INT、HUP，避免留下孤兒程序。
- 無參數介面的腳本不實作 argc rejection、`-h`、`--help` 或 `usage`；有真實參數介面時才加入最小必要解析。不另外定義 `--verbose`，debug 輸出由 `VERBOSE=1|yes|true` 控制 `log_debug`。

### Library

- Library 只提供函式、常數或必要狀態；不設定 caller shell options，不執行下載、安裝或其他自動副作用。
- 重複載入 guard、`*_LOADED=1` 與載入依賴所需的 bootstrap 變數放在 section 外。
- Library 不定義或執行 `main`，也不加入 `# Run`；單純載入依賴不視為 Run。

### POSIX sh 例外

- Bash 使用 `source`；POSIX `sh` 使用 `.`。
- POSIX maintainer scripts 只可依其契約調整 header 與 source 方式；其餘結構、平台、安全、secret、錯誤處理與產物規則仍然適用。

### 檢查與驗證

- 不預檢：下一個指令本來就會立即且明確失敗時，不在其前做等價的 `-f`、`-x`、`command -v` 或 `require_*` 檢查。
- 預檢：只用於條件分支，或在昂貴、長時間、不可逆或會留下部分副作用的流程開始前提早失敗；此時使用 `require_cmd`、`require_file` 等共用 helper，並集中在流程開頭一次完成。
- 每個必要且不同的 invariant 只驗證一次，例如 checksum、可執行性、`--version`、HTTP 最終狀態、ELF/static 屬性或 package 可解析性；後續直接使用已涵蓋相同 invariant 時不重複檢查。
- 不支援的平台必須在下載、安裝或修改檔案前報錯並以非零狀態退出。

### 錯誤處理

- 錯誤不吞：不使用無證據的 silent fallback；除非契約明確要求，不加入自動 retry 或 rollback。
- 相容 fallback、刻意忽略錯誤與特殊 trap 必須有明確理由。`|| true` 只用於失敗明確屬於非致命的 cleanup、package lifecycle 或 terminal recovery，不得套用於主要產物或主要操作。

### 依賴、下載與產物

- 直接下載固定 release/archive 時 pin 版本並驗證上游 SHA-256 或 signature；刻意下載 latest archive 時先解析具體版本並驗證該版本的上游 checksum。追蹤 Git moving branch 時，clone/fetch 後記錄實際 commit 即可，不使用同一 upstream 的 `ls-remote` 重複預查；需要 reproducible build 時才 pin commit。Package manager 安裝遵循其 lock 與驗證機制。
- 使用 `mktemp` 建立唯一暫存路徑；script-owned 暫存資源必須在退出前移除或移交，失敗可能殘留時使用 EXIT cleanup，不要求 PID 出現在路徑。
- 持久產物只要不能接受 partial update，就在目的地同一 filesystem staging 後 atomic replace。

### 安全、輸出與驗證

- Secret 建立前使用 `umask 077`，並明確將 private parent directory 設為 `0700`、secret file 設為 `0600`，不依賴 umask 結果。
- 錯誤訊息送至 stderr；Secret value 不透過 argv 或 log 傳遞，優先使用 stdin、file descriptor 或權限受限的檔案，只有工具契約要求時才使用環境變數。
- Release signing identity 不自動覆蓋；若腳本明確定義為破壞性重新產生器，執行前 warning 必須說清楚，名稱或文件不得誤導為保留既有 identity。
- 所有專案自有 Bash script 必須直接或透過共用 library 載入共用 log library，並使用 `log_debug`、`log_info`、`log_success`、`log_warn`、`log_error`；只有因執行契約而無法 source library 的獨立 script，才可直接輸出或在檔案內定義最小 logging function，且必須說明原因。
- Log 訊息使用簡潔的 English sentence case 並保留產品官方大小寫；進行中訊息使用現在進行式並以 `...` 結尾，單行成功或錯誤訊息不加句號，路徑或值前使用冒號，相關子句使用分號，避免不必要的括號與驚嘆號。
- Shell 續行參數保持一致層級。提交前依 `.editorconfig` 格式化受影響檔案，並執行對應的 `bash -n` 或 `sh -n`、`shfmt -d`、ShellCheck（可用時）與 `git diff --check`。
- JSON/YAML template 驗證 rendered output；其他 machine-readable 設定使用對應 parser 驗證。
