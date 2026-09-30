# gcode — Natural Language to Shell Command Generator

> **A local-first, offline, cross-platform CLI tool that translates natural
> language into precise shell commands using session history as context.**
>
> من الصفر إلى أول أمر في **30 ثانية**. من أداة فردية إلى **منصة عملاقة**.

---

## 📑 فهرس المحتويات

1. [نظرة عامة](#1-نظرة-عامة)
2. [الأهداف وغير الأهداف](#2-الأهداف-وغير-الأهداف)
3. [المبادئ التصميمية](#3-المبادئ-التصميمية)
4. [المنصات المدعومة](#4-المنصات-المدعومة)
5. [المعمارية التقنية](#5-المعمارية-التقنية)
6. [التقنيات المستخدمة](#6-التقنيات-المستخدمة)
7. [هيكل المستودع](#7-هيكل-المستودع)
8. [التثبيت والتشغيل السريع](#8-التثبيت-والتشغيل-السريع)
9. [الميزات الخارقة](#9-الميزات-الخارقة)
10. [خطة التنفيذ المرحلية](#10-خطة-التنفيذ-المرحلية)
11. [إدارة النموذج](#11-إدارة-النموذج)
12. [استراتيجية الاختبار](#12-استراتيجية-الاختبار)
13. [CI/CD Pipeline](#13-cicd-pipeline)
14. [الأمان والخصوصية](#14-الأمان-والخصوصية)
15. [التوثيق](#15-التوثيق)
16. [المخاطر والتخفيف](#16-المخاطر-والتخفيف)
17. [مسار النمو المؤسسي](#17-مسار-النمو-المؤسسي)
18. [نموذج العمل](#18-نموذج-العمل)
19. [مؤشرات النجاح](#19-مؤشرات-النجاح)
20. [خارطة الطريق المستقبلية](#20-خارطة-الطريق-المستقبلية)
21. [نصائح استراتيجية](#21-نصائح-استراتيجية)

---

## 1. نظرة عامة

**gcode** هي أداة سطر أوامر مفتوحة المصدر تتيح للمستخدمين وصف ما يريدون
بلغة طبيعية، فتُولّد الأداة الأمر الدقيق المطلوب — مستخدمةً سجل الجلسة
(الأوامر ومخرجاتها) كسياق لتحسين الدقة.

**مثال:**

```bash
$ gcode -c find to me all files size big than 10GB in system

🔍 Analyzing request with context from last 15 commands...
📝 Generated command:

   find / -type f -size +10G -exec ls -lh {} \; 2>/dev/null

⚠️  Risk level: MEDIUM (recursive search from root, may be slow)
Execute? [y/N/e/c]:
```

**المبادئ الأساسية:**

- 🔒 **محلي بالكامل**: كل شيء يعمل على جهاز المستخدم، بدون APIs سحابية
- ⚡ **سريع**: أقل من ثانية للاستدلال على CPU
- 🧠 **واعٍ بالسياق**: يقرأ سجل الأوامر الأخيرة ومخرجاتها
- 🛡️ **آمن**: يصنّف المخاطر قبل التنفيذ
- 🌍 **متعدد المنصات**: Linux (كل التوزيعات) + macOS (Intel & ARM)
- 🎯 **صفر تعقيد**: تثبيت بأمر واحد، تشغيل فوري

---

## 2. الأهداف وغير الأهداف

### ✅ الأهداف (v1.0)

- [x] تحويل اللغة الطبيعية إلى أوامر shell
- [x] إصلاح الأوامر الفاشلة من مخرجات الخطأ
- [x] إكمال الأوامر نصف المكتوبة
- [x] قراءة سجل الأوامر ومخرجاتها كسياق
- [x] تصنيف المخاطر + تأكيد المستخدم
- [x] العمل offline بنموذج محلي (Kitty-bash-llm, 398 MB)
- [x] تثبيت بأمر واحد عبر `curl | sh`
- [x] حزم لكل المنصات (deb, rpm, AUR, AppImage, Homebrew)
- [x] 100% مفتوح المصدر (MIT)

### ❌ غير الأهداف (v1.0)

- ❌ تطبيق بواجهة رسومية
- ❌ دعم Windows (مؤجل لـ v2.0)
- ❌ تكامل LLM سحابي (مؤجل كخيار في v1.5)
- ❌ وضع محادثة متعدد الجولات (مؤجل لـ v1.5)
- ❌ دعم Fish shell (مؤجل لـ v1.1)
- ❌ تدريب النماذج (خارج النطاق)

---

## 3. المبادئ التصميمية

| المبدأ | التطبيق |
|---|---|
| **Zero-Friction** | تثبيت بأمر واحد، بدون تسجيل، بدون API keys |
| **Local-First** | لا تتبع، لا إرسال بيانات، لا اعتماد على الشبكة |
| **Safe-by-Default** | تصنيف مخاطر + تأكيد قبل التنفيذ |
| **Simple-by-Default** | القيم الافتراضية هي الأفضل |
| **Powerful-on-Demand** | ميزات متقدمة خلف flags اختيارية |
| **Extensible** | دعم أي نموذج GGUF، نظام إضافات WASM |
| **Enterprise-Ready** | SSO، RBAC، Audit logs، Compliance |

---

## 4. المنصات المدعومة

### 4.1 أنظمة التشغيل

| المنصة | المعمارية | الأولوية | دعم v1.0 |
|---|---|---|---|
| Ubuntu / Debian | x86_64 | P0 | ✅ `.deb` |
| Fedora / RHEL | x86_64 | P0 | ✅ `.rpm` |
| Arch Linux | x86_64 | P0 | ✅ AUR |
| Alpine Linux | x86_64 | P1 | ✅ Static binary |
| Linux عام | x86_64 | P0 | ✅ AppImage |
| macOS (Apple Silicon) | arm64 | P0 | ✅ Homebrew |
| macOS (Intel) | x86_64 | P1 | ✅ Homebrew |
| Linux عام | arm64 | P2 | ✅ Static binary |

### 4.2 القشور المدعومة

| Shell | Linux | macOS | v1.0 | v1.1 |
|---|---|---|---|---|
| Bash | ✅ | ✅ | ✅ | ✅ |
| Zsh | ✅ | ✅ | ✅ | ✅ |
| Fish | ✅ | ✅ | ⏳ | ✅ |
| Nushell | ✅ | ✅ | ⏳ | ⏳ |

---

## 5. المعمارية التقنية

### 5.1 التدفق عالي المستوى

```
┌───────────────────────────────────────────────────────────────────┐
│                    إدخال المستخدم (لغة طبيعية)                    │
│              gcode -c "find files larger than 10GB"               │
└────────────────────────────┬──────────────────────────────────────┘
                             ↓
┌───────────────────────────────────────────────────────────────────┐
│  CLI PARSER (clap)                                                │
│  - flags: -c, --context, --dry-run, --model, --no-history         │
└────────────────────────────┬──────────────────────────────────────┘
                             ↓
┌───────────────────────────────────────────────────────────────────┐
│  CONTEXT BUILDER                                                  │
│  - يقرأ ~/.gcode/history.jsonl (آخر N أمرًا + مخرجاتها)          │
│  - يقرأ cwd + git status + OS info                                │
│  - يبني prompt منظم                                               │
└────────────────────────────┬──────────────────────────────────────┘
                             ↓
┌───────────────────────────────────────────────────────────────────┐
│  INFERENCE ENGINE (llama.cpp via FFI)                             │
│  - يحمّل GGUF model (Kitty-bash-llm Q4_K_M)                       │
│  - grammar-constrained decoding (tree-sitter-bash)                │
│  - يُخرج الأمر الخام                                              │
└────────────────────────────┬──────────────────────────────────────┘
                             ↓
┌───────────────────────────────────────────────────────────────────┐
│  SAFETY CLASSIFIER                                                │
│  - pattern matching (rm -rf, dd, mkfs, chmod 777, ...)            │
│  - مستويات الخطر: SAFE / LOW / MEDIUM / HIGH / CRITICAL           │
│  - شرح السبب                                                      │
└────────────────────────────┬──────────────────────────────────────┘
                             ↓
┌───────────────────────────────────────────────────────────────────┐
│  تأكيد المستخدم                                                   │
│  [y] execute  [n] cancel  [e] edit  [c] copy  [?] explain         │
└────────────────────────────┬──────────────────────────────────────┘
                             ↓
┌───────────────────────────────────────────────────────────────────┐
│  التنفيذ + التسجيل                                                │
│  - يشغّل الأمر عبر `sh -c`                                        │
│  - يلتقط stdout/stderr/exit code                                  │
│  - يضيف إلى ~/.gcode/history.jsonl                                │
└───────────────────────────────────────────────────────────────────┘
```

### 5.2 مخطط المكونات

```
┌─────────────────────────────────────────────────────────────┐
│                      ثنائي gcode                            │
│                                                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │   CLI    │→ │ Context  │→ │Inference │→ │  Safety  │   │
│  │  Parser  │  │ Builder  │  │  Engine  │  │Classifier│   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │
│       ↓             ↓              ↓              ↓         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │  clap    │  │ history  │  │llama.cpp │  │ patterns │   │
│  │          │  │  .rs     │  │   FFI    │  │  .rs     │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  Model Manager (تحميل، تحقق، تخزين مؤقت)             │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  Config Manager (~/.config/gcode/config.toml)        │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### 5.3 تدفق التقاط السجل

```
┌─────────────────────────────────────────────────────────────┐
│  Shell (bash/zsh)                                           │
│                                                             │
│  PROMPT_COMMAND / precmd hook يستدعي:                       │
│    _gcode_capture                                           │
│      ├── يقرأ: $? (exit code)                               │
│      ├── يقرأ: $(history 1)                                 │
│      ├── يقرأ: captured stdout/stderr buffer                │
│      └── يضيف سطر JSON إلى ~/.gcode/history.jsonl           │
└─────────────────────────────────────────────────────────────┘
                             ↓
┌─────────────────────────────────────────────────────────────┐
│  ~/.gcode/history.jsonl                                     │
│                                                             │
│  {"ts":1699999999,"cmd":"ls -la","exit":0,"out":"..."}     │
│  {"ts":1700000001,"cmd":"cd /tmp","exit":0,"out":""}       │
│  {"ts":1700000005,"cmd":"find / -size +10G","exit":1,...}  │
└─────────────────────────────────────────────────────────────┘
                             ↓
┌─────────────────────────────────────────────────────────────┐
│  gcode يقرأ آخر N مدخلًا عند الاستدعاء                      │
└─────────────────────────────────────────────────────────────┘
```

---

## 6. التقنيات المستخدمة

| الطبقة | الاختيار | السبب |
|---|---|---|
| **اللغة** | Rust 1.75+ | ثنائي واحد، بدء سريع، أمان ذاكرة |
| **CLI parsing** | `clap` v4 | المعيار الفعلي، derive macros |
| **الاستدلال** | `llama-cpp-rs` (FFI) | روابط llama.cpp، دعم GGUF |
| **النموذج** | Kitty-bash-llm Q4_K_M | 398 MB، 0.5B، متخصص في Bash |
| **Grammar** | `llguidance` + `tree-sitter-bash` | Constrained decoding |
| **الإعدادات** | `toml` + `serde` | مقروء بشريًا |
| **تخزين السجل** | JSONL (append-only) | بسيط، greppable، بلا DB |
| **HTTP** | `reqwest` (rustls) | تحميل النموذج |
| **التسجيل** | `tracing` + `tracing-subscriber` | structured logs |
| **معالجة الأخطاء** | `anyhow` + `thiserror` | أنواع أخطاء مريحة |
| **الاختبار** | `cargo test` + `insta` | سريع، idiomatic |
| **CI/CD** | GitHub Actions | مجاني للمشاريع المفتوحة |
| **التغليف** | `cargo-dist` + scripts | أتمتة متعددة الصيغ |

---

## 7. هيكل المستودع

```
gcode/
├── .github/
│   ├── workflows/
│   │   ├── ci.yml
│   │   ├── release.yml
│   │   └── model-check.yml
│   ├── ISSUE_TEMPLATE/
│   │   ├── bug_report.md
│   │   └── feature_request.md
│   └── PULL_REQUEST_TEMPLATE.md
├── src/
│   ├── main.rs
│   ├── cli.rs
│   ├── config.rs
│   ├── context/
│   │   ├── mod.rs
│   │   ├── history.rs
│   │   ├── prompt.rs
│   │   └── env.rs
│   ├── inference/
│   │   ├── mod.rs
│   │   ├── engine.rs
│   │   ├── grammar.rs
│   │   └── server.rs
│   ├── model/
│   │   ├── mod.rs
│   │   ├── download.rs
│   │   └── registry.rs
│   ├── safety/
│   │   ├── mod.rs
│   │   ├── patterns.rs
│   │   └── classifier.rs
│   ├── exec/
│   │   ├── mod.rs
│   │   ├── runner.rs
│   │   └── capture.rs
│   └── utils/
│       ├── mod.rs
│       └── paths.rs
├── shell/
│   ├── gcode.bash
│   ├── gcode.zsh
│   └── capture.bash
├── models/
│   └── registry.toml
├── packaging/
│   ├── debian/
│   ├── rpm/
│   ├── arch/
│   ├── appimage/
│   └── homebrew/
├── docs/
│   ├── INSTALL.md
│   ├── USAGE.md
│   ├── MODELS.md
│   ├── SAFETY.md
│   ├── ARCHITECTURE.md
│   └── CONTRIBUTING.md
├── tests/
│   ├── integration/
│   └── fixtures/
├── benches/
│   └── inference_bench.rs
├── scripts/
│   ├── install.sh
│   ├── build-all.sh
│   └── verify-release.sh
├── .gitignore
├── .rustfmt.toml
├── .clippy.toml
├── Cargo.toml
├── Cargo.lock
├── LICENSE
├── README.md
├── CHANGELOG.md
├── SECURITY.md
└── plan.md
```

---

## 8. التثبيت والتشغيل السريع

### 8.1 المبدأ: "من الصفر إلى أول أمر في 30 ثانية"

الهدف: **يجب أن يحصل المستخدم على أول أمر ناجح خلال 30 ثانية**، بدون:

- ❌ قراءة توثيق طويل
- ❌ إعداد مفاتيح API
- ❌ تثبيت Python/Rust/اعتماديات
- ❌ تعديل ملفات إعدادات يدويًا
- ❌ إعادة تشغيل الجلسة

### 8.2 الأمر الواحد الشامل

```bash
curl -fsSL https://get.gcode.dev | sh
```

**ما يفعله السكربت تلقائيًا:**

```
┌──────────────────────────────────────────────────────────────┐
│  1. يكتشف النظام (Ubuntu/Debian/Fedora/Arch/macOS/Alpine)   │
│  2. يكتشف المعمارية (x86_64 / arm64)                        │
│  3. يحمّل الثنائي المناسب (~8 MB)                           │
│  4. يثبته في /usr/local/bin (أو ~/.local/bin بدون root)     │
│  5. يحمّل النموذج تلقائيًا (~398 MB) في الخلفية             │
│  6. يضيف تكامل القشرة (bash/zsh) تلقائيًا                   │
│  7. يشغّل اختبار سريع للتأكد من عمله                        │
│  8. يعرض: "✓ Ready. Try: gcode -c 'list large files'"       │
└──────────────────────────────────────────────────────────────┘
```

**السكربت الفعلي (`scripts/install.sh`):**

```bash
#!/bin/sh
set -e

# ─── اكتشاف النظام والمعمارية ─────────────────────────────────
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    arm64|aarch64) ARCH="aarch64" ;;
    *) echo "Unsupported arch: $ARCH"; exit 1 ;;
esac

# ─── اختيار target triple ─────────────────────────────────────
case "$OS" in
    linux)  TARGET="${ARCH}-unknown-linux-musl" ;;   # static
    darwin) TARGET="${ARCH}-apple-darwin" ;;
    *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

# ─── مجلد التثبيت (بدون sudo إن أمكن) ────────────────────────
if [ -w /usr/local/bin ]; then
    BIN_DIR=/usr/local/bin
elif [ -w "$HOME/.local/bin" ] || mkdir -p "$HOME/.local/bin"; then
    BIN_DIR="$HOME/.local/bin"
    export PATH="$BIN_DIR:$PATH"
else
    BIN_DIR=/usr/local/bin
    SUDO=sudo
fi

# ─── تحميل الثنائي ────────────────────────────────────────────
VERSION=$(curl -fsSL https://api.github.com/repos/yourname/gcode/releases/latest | grep tag_name | cut -d'"' -f4)
URL="https://github.com/yourname/gcode/releases/download/${VERSION}/gcode-${TARGET}.tar.gz"

echo "Downloading gcode ${VERSION} for ${TARGET}..."
curl -fsSL "$URL" | tar -xz -C /tmp
${SUDO} install -m755 /tmp/gcode "$BIN_DIR/gcode"
rm -f /tmp/gcode

# ─── تحميل النموذج في الخلفية ────────────────────────────────
echo "Preparing model (398 MB) in background..."
"$BIN_DIR/gcode" --download-model --quiet &

# ─── تكامل القشرة ─────────────────────────────────────────────
"$BIN_DIR/gcode" --init --quiet

# ─── اختبار سريع ──────────────────────────────────────────────
echo ""
echo "✓ gcode installed successfully!"
echo ""
echo "  Quick start:"
echo "    gcode -c \"list all files larger than 1GB\""
echo ""
```

### 8.3 الأوامر الأساسية

```bash
# الأمر الأساسي — يعمل مباشرة بعد التثبيت
gcode -c "أريد حذف كل الملفات الأكبر من 10GB في /tmp"

# ترجمة فورية دون تنفيذ (للمراجعة)
gcode -c "..." --dry-run

# إصلاح الأمر الأخير الذي فشل
gcode --fix

# إكمال أمر نصف مكتوب
gcode --complete "find /var/log -type f -name"

# إعداد تكامل القشرة
gcode --init
```

### 8.4 الواجهة التفاعلية الاختيارية

```
$ gcode
┌─────────────────────────────────────────────────────────┐
│  gcode v1.0 — Natural Language Shell                    │
│  Type your request, or Ctrl+D to exit                   │
└─────────────────────────────────────────────────────────┘

> find me all files > 10GB

  ⚡ find / -type f -size +10G 2>/dev/null
  ⚠ Risk: MEDIUM (recursive from root)
  → [y]es / [n]o / [e]dit / [c]opy / [?]explain / [r]efine
```

### 8.5 التثبيت عبر مديري الحزم

| النظام | الأمر |
|---|---|
| **Debian/Ubuntu** | `sudo apt install gcode` |
| **Fedora** | `sudo dnf install gcode` |
| **Arch** | `yay -S gcode` |
| **Alpine** | `apk add gcode` |
| **macOS** | `brew install gcode` |
| **Nix** | `nix-env -iA nixpkgs.gcode` |
| **Docker** | `docker run -it --rm gcode/gcode` |
| **بدون تثبيت** | `curl -fsSL https://get.gcode.dev \| sh` |

### 8.6 مبدأ "لا تُجبر المستخدم على شيء"

- **لا تسجيل** — الأداة تعمل بدون حساب
- **لا API key** — النموذج محلي
- **لا إعدادات إلزامية** — تعمل بالقيم الافتراضية
- **لا تعديل ملفات** — `--init` يفعل ذلك فقط بطلب صريح
- **لا بيانات تغادر الجهاز** — بدون استثناء

---

## 9. الميزات الخارقة

هذه الميزات ستحوّل `gcode` من "أداة جيدة" إلى **"أداة لا يمكن الاستغناء عنها"**.

### 🌟 الفئة أ: الذكاء التوليدي المتقدم

#### A1. Agentic Multi-Step Execution (التنفيذ الوكيلي)

```bash
$ gcode -c "setup a new Django project with postgres, redis, and deploy to staging"

📋 Generated plan (7 steps):

  1. python3 -m venv .venv && source .venv/bin/activate
  2. pip install django psycopg2-binary redis gunicorn
  3. django-admin startproject config .
  4. psql -c "CREATE DATABASE myapp;"
  5. docker run -d --name redis -p 6379:6379 redis:alpine
  6. python manage.py migrate && python manage.py createsuperuser
  7. gunicorn config.wsgi:application --bind 0.0.0.0:8000

  ⚠ Steps 4, 5 require Docker/Postgres running
  → [r]un all / [s]tep-by-step / [e]dit / [a]bort
```

**التنفيذ:** نموذج planner ثانوي + فحص النتيجة بعد كل خطوة + rollback.

#### A2. Self-Healing Commands (الإصلاح الذاتي)

```bash
$ gcode -c "install nodejs 20"
→ apt install nodejs=20
✗ Error: Version '20' not found for 'nodejs'

$ gcode --fix
🔍 Analyzing error...
📝 Fixed command:
   curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
   && sudo apt install -y nodejs

→ Execute? [y/N]
```

#### A3. Learning from Edits (التعلم من التعديلات)

```toml
# ~/.config/gcode/learned.toml
[[pattern]]
trigger = "list large files"
generated = "find . -size +100M"
user_edited = "find . -size +100M -not -path './node_modules/*'"
confidence = 0.92
```

بعد 5 تعديلات متشابهة، تصبح النسخة المعدّلة هي الافتراضية.

#### A4. Conversational Session Mode

```bash
$ gcode --chat
> list all docker containers
  ⚡ docker ps -a
> now stop the ones older than 30 days
  ⚡ docker ps -a --filter "status=exited" --format "{{.ID}} {{.Status}}" \
     | awk '{...}' | xargs docker rm
> make a backup script that runs this daily
  ⚡ [generates full backup.sh with cron setup]
```

#### A5. Natural Language Pipelines

```bash
$ gcode -c "download example.com/data.zip, extract it, filter CSVs with > 1000 rows, compress, and upload to S3"

⚡ curl -sSL example.com/data.zip -o data.zip \
  && unzip -q data.zip -d data/ \
  && find data/ -name "*.csv" -exec sh -c 'wc -l "$1" | awk "\$1>1000"' _ {} \; \
  && tar -czf filtered.tar.gz data/ \
  && aws s3 cp filtered.tar.gz s3://bucket/
```

### 🌟 الفئة ب: الأمان والثقة

#### B1. Sandbox Preview Mode

```bash
$ gcode -c "delete old log files" --sandbox
🧪 Running in sandbox (container: gcode-sandbox-abc123)...

  Container state BEFORE: 2.3 GB logs
  Container state AFTER:  145 MB logs
  Files affected: 1,247
  Estimated time: 3.2 seconds

  ⚠ In sandbox: no files older than 7 days were deleted
    (test container has no old files)

→ Execute on real system? [y/N]
```

#### B2. Rollback Engine

```bash
$ gcode --rollback last
↩ Rolling back: rm -rf ~/projects/old
  Restored: 47 files from snapshot
  Status: ✓ Complete
```

**كيف يعمل:**
- يستخدم `btrfs`/`zfs` snapshots تلقائيًا
- أو `git` للملفات النصية
- أو نسخ في `~/.gcode/snapshots/`

#### B3. Compliance Mode

```toml
# /etc/gcode/enterprise.toml
[compliance]
mode = "strict"
audit_log = "/var/log/gcode/audit.jsonl"
require_approval_for = ["critical", "high"]
forbidden_commands = ["rm -rf /", "mkfs", "dd of=/dev/*"]
allowed_users = ["@engineering"]
retention_days = 365
```

**Audit log لكل أمر:**

```jsonl
{
  "ts": "2025-01-15T14:23:11Z",
  "user": "mohammed@company.com",
  "host": "prod-web-01",
  "request": "delete old logs",
  "generated": "find /var/log -mtime +30 -delete",
  "risk": "medium",
  "approved_by": "mohammed@company.com",
  "executed": true,
  "exit_code": 0,
  "duration_ms": 1240
}
```

#### B4. Policy Engine

```rego
package gcode.policy

deny[msg] {
    input.command.contains("rm -rf /")
    msg := "Catastrophic delete blocked"
}

deny[msg] {
    input.command.matches(".*drop\\s+database.*")
    not input.user.groups.contains("dba")
    msg := "Only DBAs can drop databases"
}

warn[msg] {
    input.command.contains("sudo")
    msg := "Elevated privileges required"
}
```

### 🌟 الفئة ج: التكامل المؤسسي

#### C1. Team Knowledge Base

```bash
$ gcode --share "deploy to production"
✓ Shared as team snippet 'prod-deploy'
  Tags: deployment, production, k8s
  Users who can use: @devops

$ gcode --search "deploy"
  1. prod-deploy (by @ali, used 47 times)
  2. staging-deploy (by @sara, used 23 times)
  3. rollback-deploy (by @mohammed, used 8 times)
```

#### C2. SSO / RBAC Integration

```toml
[auth]
provider = "okta"          # أو "azure-ad", "google", "keycloak"
client_id = "..."
tenant = "company.okta.com"

[rbac]
admin = ["@platform-team"]
developer = ["@engineering"]
readonly = ["@support"]
```

#### C3. IDE Integration

- **VS Code**: Command Palette → `gcode`
- **JetBrains**: Tool window + shortcut
- **Neovim**: `:Gcode` command
- **Emacs**: `M-x gcode`

#### C4. CI/CD Integration

**GitHub Action:**

```yaml
- uses: yourname/gcode-action@v1
  with:
    request: "deploy to staging"
    dry-run: true
```

**GitLab CI:**

```yaml
deploy:
  script:
    - gcode -c "deploy to staging" --yes
```

### 🌟 الفئة د: الذكاء المتخصص

#### D1. Domain-Aware Models

| المجال | النموذج | الحجم |
|---|---|---|
| **عام** | Kitty-bash-llm | 398 MB |
| **DevOps / K8s** | gcode-k8s | 450 MB |
| **AWS / Cloud** | gcode-aws | 480 MB |
| **DB / SQL** | gcode-sql | 420 MB |
| **Security** | gcode-sec | 500 MB |
| **Data Engineering** | gcode-data | 450 MB |

```bash
gcode --use-model gcode-k8s
gcode -c "scale my deployment to 5 replicas with autoscaling"
```

#### D2. Cost Estimation

```bash
$ gcode -c "spin up 10 EC2 instances for ML training"
💡 Estimated cost:
   EC2 (m5.xlarge × 10): $1.92/hour = $46/day = $1,380/month
   Storage (100 GB × 10): $10/month
   Total monthly: ~$1,390
→ Proceed? [y/N]
```

#### D3. Incident Response Mode

```bash
$ gcode --incident "server is slow"
🔍 Investigating...

  Runbook: server-slow.md
  ├─ 1. Check CPU:     top -bn1 | head -20
  ├─ 2. Check memory:  free -h
  ├─ 3. Check disk:    df -h
  ├─ 4. Check IO:      iostat -x 1 3
  └─ 5. Check network: ss -tulpn

  Auto-run all? [y/N]  Or [s]tep by step?
```

#### D4. Screen Context

```bash
$ gcode --with-screen -c "fix this error"
📸 Captured screen context
🔍 Detected error in terminal:
   "ImportError: No module named 'requests'"
📝 Generated command:
   pip install requests
```

#### D5. Voice Input

```bash
$ gcode --voice
🎤 Listening... (speak now)
> "find all files modified today and copy them to backup folder"
⚡ find . -type f -mtime -1 -exec cp {} ~/backup/ \;
```

### 🌟 الفئة هـ: الأداء والتوسع

#### E1. Daemon Mode

```bash
$ systemctl --user enable --now gcode-daemon
# الاستدعاءات التالية أسرع بـ 5-10 مرات
$ time gcode -c "list files"
real    0m0.089s    # 89ms بدل 800ms
```

#### E2. GPU Acceleration

دعم تلقائي لـ:
- **NVIDIA** (CUDA)
- **AMD** (ROCm)
- **Apple Silicon** (Metal)
- **Intel** (oneAPI)

```bash
$ gcode --gpu auto
🎮 Detected: Apple M3 Max (40-core GPU)
⚡ Inference: 25 tokens/sec (CPU) → 180 tokens/sec (GPU)
```

#### E3. Model Marketplace

```bash
$ gcode --browse-models
┌────────────────────────────────────────────────────────┐
│  Popular Models                                        │
├────────────────────────────────────────────────────────┤
│  ★ kitty-bash-llm       398 MB   Bash   Official      │
│    gcode-k8s            450 MB   K8s    Verified      │
│    bash-guru            520 MB   Bash   Community     │
│    devops-pro           610 MB   Multi  Community     │
└────────────────────────────────────────────────────────┘
```

#### E4. Federated Learning (opt-in)

تحسين النموذج عبر مساهمات مجهولة من مستخدمين موافقين، **بدون إرسال أوامر حقيقية**، فقط أنماط لغوية.

### 🌟 الفئة و: ميزات ثورية

#### F1. Time-Travel Shell

```bash
$ gcode --timeline "yesterday at 3pm"
📅 Restoring shell context from 2025-01-14 15:00
   Commands run: 47
   Current dir: /var/www/app
   Env vars: (snapshot)
→ Continue from there? [y/N]
```

#### F2. Cross-Shell Translation

```bash
$ gcode --translate "for f in *.txt; do wc -l $f; done" --to fish
⚡ for f in *.txt; wc -l $f; end
```

يدعم: Bash ↔ Zsh ↔ Fish ↔ PowerShell ↔ Nushell

#### F3. Natural Language Cron

```bash
$ gcode --cron "every day at 3am backup database to S3"
⚡ Generated crontab:
   0 3 * * * /usr/local/bin/backup-db.sh >> /var/log/backup.log 2>&1

→ Install? [y/N]
```

#### F4. Command Explainer

```bash
$ gcode --explain "awk -F: '{print \$1}' /etc/passwd | sort -u"
📖 Explanation:
   - awk -F: → splits each line by ':'
   - '{print $1}' → prints first field (usernames)
   - sort -u → sorts and removes duplicates
   → Result: list of unique usernames
```

#### F5. Shell Autopsy

```bash
$ gcode --autopsy last-hour
📊 Session Analysis (last 60 min):
   Commands: 47
   Failed: 8 (17%)
   Most used: git (12), npm (7), docker (5)
   Patterns detected:
     - Repeatedly fixing npm install (3 times)
     - Suggestion: pin your node version
```

---

## 10. خطة التنفيذ المرحلية

| المرحلة | المدة | المُخرَج | يعطّل التالية |
|---|---|---|---|
| **0. الأساس** | أسبوع 1 | سقالة المستودع، CI أخضر | نعم |
| **1. المحرك الأساسي** | أسبوعان | `gcode -c "..."` يعمل | نعم |
| **2. تكامل القشرة** | أسبوع 4 | سياق السجل يعمل | نعم |
| **3. طبقة الأمان** | أسبوع 5 | تصنيف + تأكيد | نعم |
| **4. التغليف** | أسبوع 6 | deb, rpm, AUR, AppImage, Homebrew | نعم |
| **5. التوزيع و CI/CD** | أسبوع 7 | مستودع APT، COPR، إصدارات آلية | نعم |
| **6. الصقل والإطلاق** | أسبوع 8 | v1.0 منشور | لا |

**الإجمالي: 8 أسابيع (بدوام جزئي، ~20 ساعة/أسبوع)**

### Phase 0 — الأساس (أسبوع 1)

#### الأهداف
- [x] إنشاء مستودع GitHub
- [x] سقالة مشروع Rust
- [x] إعداد CI (build + test + lint)
- [x] إعداد الترخيص، README، CONTRIBUTING

#### المهام

**0.1 إعداد المستودع**

```bash
cargo new --bin gcode
cd gcode
git init
git remote add origin git@github.com:yourname/gcode.git
```

**0.2 `Cargo.toml`**

```toml
[package]
name = "gcode"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
license = "MIT"
description = "Natural language to Bash command generator"
repository = "https://github.com/yourname/gcode"
keywords = ["cli", "bash", "llm", "shell", "ai"]
categories = ["command-line-utilities"]

[[bin]]
name = "gcode"
path = "src/main.rs"

[dependencies]
clap = { version = "4.5", features = ["derive", "env"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"
anyhow = "1.0"
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
reqwest = { version = "0.11", default-features = false, features = ["rustls-tls", "blocking"] }
sha2 = "0.10"
dirs = "5.0"
regex = "1.10"
colored = "2.1"
dialoguer = "0.11"
llama-cpp-rs = { version = "0.3", optional = true }
llguidance = "0.4"

[features]
default = ["llama"]
llama = ["llama-cpp-rs"]

[dev-dependencies]
insta = "1.36"
tempfile = "3.10"
assert_cmd = "2.0"
predicates = "3.1"

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
panic = "abort"
```

**0.3 CI Workflow (`.github/workflows/ci.yml`)**

```yaml
name: CI
on: [push, pull_request]

jobs:
  test:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, macos-14]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test --all-features
      - run: cargo build --release
```

#### معايير القبول
- [ ] `cargo build --release` ينجح على Linux + macOS
- [ ] `cargo test` ينجح
- [ ] CI أخضر على أول PR
- [ ] README مع وصف المشروع
- [ ] MIT LICENSE

### Phase 1 — المحرك الأساسي (أسابيع 2–3)

#### الأهداف
- [x] CLI parsing مع `clap`
- [x] تحميل النموذج + التحقق
- [x] تكامل llama.cpp FFI
- [x] استدلال أساسي: `gcode -c "..."` → command
- [x] Grammar-constrained decoding

#### المهام

**1.1 تعريف CLI (`src/cli.rs`)**

```rust
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "gcode", version, about = "Natural language to Bash")]
pub struct Cli {
    #[arg(short = 'c', long = "command")]
    pub request: Option<String>,

    #[arg(long)]
    pub fix: bool,

    #[arg(long)]
    pub complete: Option<String>,

    #[arg(long, env = "GCODE_MODEL")]
    pub model: Option<PathBuf>,

    #[arg(long)]
    pub no_history: bool,

    #[arg(long, default_value_t = 15)]
    pub history_depth: usize,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(short = 'y', long)]
    pub yes: bool,

    #[arg(long)]
    pub init: bool,
}
```

**1.2 سجل النماذج (`models/registry.toml`)**

```toml
[[models]]
name = "kitty-bash-llm"
display = "Kitty-bash-llm (Q4_K_M)"
size = 398_000_000
sha256 = "abc123..."
url = "https://huggingface.co/sahellx/kitty-bash-llm/resolve/main/kitty-bash-llm-Q4_K_M.gguf"
default = true
description = "0.5B Qwen2.5-Coder fine-tune for Bash"

[[models]]
name = "bashgemma"
display = "BashGemma (270M)"
size = 540_000_000
sha256 = "def456..."
url = "https://huggingface.co/.../bashgemma-Q4_K_M.gguf"
default = false
description = "270M FunctionGemma fine-tune"
```

**1.3 تحميل النموذج (`src/model/download.rs`)**

```rust
pub fn ensure_model(name: &str) -> Result<PathBuf> {
    let dir = dirs::data_dir()
        .context("no data dir")?
        .join("gcode/models");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.gguf", name));

    if path.exists() && verify_checksum(&path, name)? {
        return Ok(path);
    }

    let entry = registry::get(name)?;
    eprintln!("Downloading {} ({} MB)...", entry.display, entry.size / 1_000_000);
    download_with_progress(&entry.url, &path)?;
    verify_checksum(&path, name)?;
    Ok(path)
}
```

**1.4 محرك الاستدلال (`src/inference/engine.rs`)**

```rust
pub struct Engine {
    ctx: LlamaContext,
    grammar: LlamaGrammar,
}

impl Engine {
    pub fn new(model_path: &Path) -> Result<Self> {
        let backend = LlamaBackend::init()?;
        let model = LlamaModel::load_from_file(&backend, model_path, &Default::default())?;
        let ctx = model.new_context(&backend, Default::default())?;
        let grammar = LlamaGrammar::from_str(BASH_GRAMMAR)?;
        Ok(Self { ctx, grammar })
    }

    pub fn generate(&mut self, prompt: &str, max_tokens: usize) -> Result<String> {
        // tokenize, sample with grammar, decode
        // ...
    }
}
```

**1.5 Bash Grammar (`src/inference/grammar.rs`)**

```rust
pub const BASH_GRAMMAR: &str = r#"
root        ::= command (" " pipe " " command)*
command     ::= binary (" " argument)*
binary      ::= [a-zA-Z0-9_/.-]+
argument    ::= quoted | unquoted
quoted      ::= "\"" [^"]* "\"" | "'" [^']* "'"
unquoted    ::= [^ \t\n|><;&]+
pipe        ::= "|" | "&&" | "||" | ";"
"#;
```

**1.6 قالب Prompt (`src/context/prompt.rs`)**

```rust
pub fn build_prompt(request: &str, history: &[HistoryEntry]) -> String {
    let mut p = String::new();
    p.push_str("<|im_start|>system\n");
    p.push_str("You are a Bash expert. Output ONLY the command, no explanation.\n");
    p.push_str("<|im_end|>\n");

    if !history.is_empty() {
        p.push_str("<|im_start|>user\nRecent shell context:\n");
        for e in history {
            p.push_str(&format!("$ {}\n# exit: {}\n", e.cmd, e.exit));
            if !e.out.is_empty() {
                p.push_str(&format!("# output: {}\n", truncate(&e.out, 500)));
            }
        }
        p.push_str("<|im_end|>\n");
    }

    p.push_str(&format!("<|im_start|>user\n{}\n<|im_end|>\n", request));
    p.push_str("<|im_start|>assistant\n");
    p
}
```

#### معايير القبول
- [ ] `gcode -c "list all files"` يُخرج `ls -la` أو ما شابه
- [ ] `gcode -c "find files > 10GB"` يُخرج أمر `find`
- [ ] النموذج يُحمّل مرة، يُخزّن مؤقتًا
- [ ] الاستدلال < 1.5s على CPU (2 cores)
- [ ] Grammar يمنع Bash غير صالح
- [ ] `--dry-run` يعرض دون تنفيذ

### Phase 2 — تكامل القشرة (أسبوع 4)

#### الأهداف
- [x] التقاط الأمر + exit code + output
- [x] تخزين في `~/.gcode/history.jsonl`
- [x] `gcode --init` لتثبيت hooks
- [x] دعم bash + zsh

#### المهام

**2.1 تنسيق تخزين السجل**

```jsonl
{"ts":1699999999,"cmd":"ls -la","exit":0,"cwd":"/home/user","out":"total 48\n..."}
{"ts":1700000001,"cmd":"cd /tmp","exit":0,"cwd":"/home/user","out":""}
{"ts":1700000005,"cmd":"find / -size +10G","exit":1,"cwd":"/tmp","out":"Permission denied\n..."}
```

**2.2 تكامل Bash (`shell/gcode.bash`)**

```bash
# gcode shell integration for bash

_gcode_capture() {
    local exit_code=$?
    local last_cmd
    last_cmd=$(HISTTIMEFORMAT= history 1 | sed 's/^ *[0-9]* *//')

    if [ -n "$_GCODE_LAST_CMD" ] && [ "$last_cmd" = "$_GCODE_LAST_CMD" ]; then
        return
    fi

    case "$last_cmd" in
        gcode*|_gcode*) return ;;
    esac

    local dir="${XDG_DATA_HOME:-$HOME/.local/share}/gcode"
    mkdir -p "$dir"

    if command -v jq >/dev/null 2>&1; then
        jq -nc \
            --arg cmd "$last_cmd" \
            --argjson exit "$exit_code" \
            --arg cwd "$PWD" \
            --argjson ts "$(date +%s)" \
            '{ts:$ts, cmd:$cmd, exit:$exit, cwd:$cwd}' \
            >> "$dir/history.jsonl"
    else
        printf '{"ts":%d,"cmd":"%s","exit":%d,"cwd":"%s"}\n' \
            "$(date +%s)" "$last_cmd" "$exit_code" "$PWD" \
            >> "$dir/history.jsonl"
    fi

    _GCODE_LAST_CMD="$last_cmd"
}

PROMPT_COMMAND="_gcode_capture${PROMPT_COMMAND:+; $PROMPT_COMMAND}"
```

**2.3 تكامل Zsh (`shell/gcode.zsh`)**

```zsh
autoload -Uz add-zsh-hook

_gcode_capture() {
    local exit_code=$?
    local last_cmd="$1"

    case "$last_cmd" in
        gcode*|_gcode*) return ;;
    esac

    local dir="${XDG_DATA_HOME:-$HOME/.local/share}/gcode"
    mkdir -p "$dir"

    printf '{"ts":%d,"cmd":"%s","exit":%d,"cwd":"%s"}\n' \
        "$(date +%s)" "$last_cmd" "$exit_code" "$PWD" \
        >> "$dir/history.jsonl"
}

add-zsh-hook preexec _gcode_preexec
_gcode_preexec() { _GCODE_LAST_CMD="$1"; }
add-zsh-hook precmd _gcode_capture
```

**2.4 `gcode --init`**

```rust
pub fn init_shell() -> Result<()> {
    let shell = std::env::var("SHELL").unwrap_or_default();
    let rc = if shell.contains("zsh") {
        dirs::home_dir().unwrap().join(".zshrc")
    } else {
        dirs::home_dir().unwrap().join(".bashrc")
    };

    let marker = "# gcode-shell";
    let content = std::fs::read_to_string(&rc).unwrap_or_default();
    if content.contains(marker) {
        println!("Already initialized in {}", rc.display());
        return Ok(());
    }

    let snippet = format!(
        "\n{}\nsource /usr/share/gcode/gcode.{}\n",
        marker,
        if shell.contains("zsh") { "zsh" } else { "bash" }
    );
    std::fs::OpenOptions::new()
        .append(true).create(true)
        .open(&rc)?
        .write_all(snippet.as_bytes())?;

    println!("✓ Added gcode to {}. Run `exec $SHELL` to activate.", rc.display());
    Ok(())
}
```

**2.5 قارئ السجل (`src/context/history.rs`)**

```rust
pub fn read_recent(depth: usize) -> Result<Vec<HistoryEntry>> {
    let path = history_path()?;
    if !path.exists() { return Ok(Vec::new()); }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut entries: Vec<HistoryEntry> = reader
        .lines()
        .filter_map(|l| l.ok())
        .filter_map(|l| serde_json::from_str(&l).ok())
        .collect();

    let start = entries.len().saturating_sub(depth);
    Ok(entries.split_off(start))
}
```

#### معايير القبول
- [ ] بعد `gcode --init`، كل أمر يُسجَّل
- [ ] `~/.gcode/history.jsonl` ينمو بشكل صحيح
- [ ] `gcode -c "..."` يضم آخر 15 أمرًا في prompt
- [ ] `--no-history` يعطّل السياق
- [ ] يعمل في bash و zsh
- [ ] لا تراجع في الأداء (< 5ms latency)

### Phase 3 — طبقة الأمان (أسبوع 5)

#### الأهداف
- [x] أنماط تصنيف المخاطر
- [x] تأكيد تفاعلي
- [x] `--dry-run`
- [x] شرح المخاطر

#### المهام

**3.1 أنماط المخاطر (`src/safety/patterns.rs`)**

```rust
pub struct Pattern {
    pub regex: Regex,
    pub level: Risk,
    pub reason: &'static str,
}

pub fn patterns() -> Vec<Pattern> {
    vec![
        // CRITICAL
        Pattern::new(r"\brm\s+-rf\s+/(\s|$)", Risk::Critical, "recursive delete from root"),
        Pattern::new(r"\bmkfs\.", Risk::Critical, "format filesystem"),
        Pattern::new(r"\bdd\s+.*of=/dev/(sd|nvme|hd)", Risk::Critical, "overwrite disk"),
        Pattern::new(r">\s*/dev/(sd|nvme|hd)", Risk::Critical, "write to raw disk"),

        // HIGH
        Pattern::new(r"\brm\s+-rf\s+~", Risk::High, "recursive delete home"),
        Pattern::new(r"\bchmod\s+-R\s+777\s+/", Risk::High, "world-writable root"),
        Pattern::new(r"\b(curl|wget)\s+.*\|\s*(sh|bash)", Risk::High, "pipe download to shell"),
        Pattern::new(r"\biptables\s+-F", Risk::High, "flush firewall rules"),
        Pattern::new(r"\b(shutdown|reboot|halt|poweroff)\b", Risk::High, "system power state"),

        // MEDIUM
        Pattern::new(r"\bsudo\b", Risk::Medium, "requires elevated privileges"),
        Pattern::new(r"\bfind\s+/\s", Risk::Medium, "recursive search from root"),
        Pattern::new(r"\bkill(all)?\s+-9", Risk::Medium, "force kill processes"),
        Pattern::new(r"\bapt(-get)?\s+(remove|purge)", Risk::Medium, "uninstall packages"),
        Pattern::new(r"\bdocker\s+(rm|rmi|system\s+prune)", Risk::Medium, "remove docker resources"),

        // LOW
        Pattern::new(r"\bmv\s+", Risk::Low, "moves files"),
        Pattern::new(r"\bcp\s+-r", Risk::Low, "recursive copy"),
        Pattern::new(r"\btruncate\s+-s\s+0", Risk::Low, "empties a file"),
    ]
}
```

**3.2 المصنّف (`src/safety/classifier.rs`)**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Risk { Safe, Low, Medium, High, Critical }

pub struct Assessment {
    pub level: Risk,
    pub reasons: Vec<String>,
}

pub fn classify(cmd: &str) -> Assessment {
    let mut level = Risk::Safe;
    let mut reasons = Vec::new();

    for p in patterns() {
        if p.regex.is_match(cmd) {
            level = level.max(p.level);
            reasons.push(p.reason.to_string());
        }
    }

    Assessment { level, reasons }
}
```

**3.3 تأكيد UI (`src/safety/prompt.rs`)**

```rust
pub fn confirm(cmd: &str, assessment: &Assessment, force_yes: bool) -> Result<Action> {
    let (icon, color) = match assessment.level {
        Risk::Safe     => ("✓", Color::Green),
        Risk::Low      => ("•", Color::Blue),
        Risk::Medium   => ("⚠", Color::Yellow),
        Risk::High     => ("⚠⚠", Color::Red),
        Risk::Critical => ("🛑", Color::Red),
    };

    println!("\n{} Risk: {:?}", icon, assessment.level);
    for r in &assessment.reasons {
        println!("   - {}", r);
    }

    if force_yes && assessment.level >= Risk::High {
        bail!("Refusing to auto-confirm high-risk command. Use without -y.");
    }
    if force_yes { return Ok(Action::Execute); }

    let choice = Select::new()
        .with_prompt("Execute?")
        .items(&["Yes", "No", "Edit", "Copy", "Explain"])
        .default(1)
        .interact()?;

    Ok(match choice {
        0 => Action::Execute,
        1 => Action::Cancel,
        2 => Action::Edit,
        3 => Action::Copy,
        _ => Action::Explain,
    })
}
```

#### معايير القبول
- [ ] `rm -rf /` يُصنَّف CRITICAL، يحجب `-y`
- [ ] `sudo apt install` MEDIUM
- [ ] `ls` SAFE
- [ ] شرح يظهر لكل أمر غير SAFE
- [ ] `--dry-run` لا ينفذ أبدًا
- [ ] اختبارات وحدة تغطي كل الأنماط

### Phase 4 — التغليف (أسبوع 6)

#### الأهداف
- [x] `.deb`
- [x] `.rpm`
- [x] AUR `PKGBUILD`
- [x] AppImage
- [x] Homebrew formula
- [x] Static binary لـ Alpine

#### المهام

**4.1 تغليف Debian**

**`packaging/debian/control`**

```debcontrol
Source: gcode
Section: utils
Priority: optional
Maintainer: Mohammed <mohammed@example.com>
Build-Depends: debhelper-compat (= 13), cargo, rustc, libclang-dev, pkg-config, libssl-dev
Standards-Version: 4.6.2
Homepage: https://github.com/yourname/gcode
Rules-Requires-Root: no

Package: gcode
Architecture: amd64 arm64
Depends: ${shlibs:Depends}, ${misc:Depends}, curl, jq
Recommends: gcode-model-kitty
Description: Natural language to Bash command generator
 gcode translates natural language requests into precise Bash commands,
 using recent shell history as context. Runs entirely offline.

Package: gcode-model-kitty
Architecture: all
Depends: gcode
Description: Kitty-bash-llm model for gcode
 Quantized Qwen2.5-Coder-0.5B fine-tune (398 MB).
```

**`packaging/debian/rules`**

```makefile
#!/usr/bin/make -f
%:
	dh $@

override_dh_auto_build:
	cargo build --release --locked
	strip --strip-all target/release/gcode

override_dh_auto_install:
	install -Dm755 target/release/gcode \
	    debian/gcode/usr/bin/gcode
	install -Dm644 shell/gcode.bash \
	    debian/gcode/usr/share/gcode/gcode.bash
	install -Dm644 shell/gcode.zsh \
	    debian/gcode/usr/share/gcode/gcode.zsh
```

**`packaging/debian/postinst`** (لـ `gcode-model-kitty`)

```bash
#!/bin/bash
set -e

MODEL_DIR="/usr/share/gcode/models"
MODEL_FILE="$MODEL_DIR/kitty-bash-llm-q4_k_m.gguf"
MODEL_URL="https://huggingface.co/sahellx/kitty-bash-llm/resolve/main/kitty-bash-llm-Q4_K_M.gguf"
MODEL_SHA256="abc123..."

mkdir -p "$MODEL_DIR"

if [ ! -f "$MODEL_FILE" ]; then
    echo "Downloading Kitty-bash-llm (~398 MB)..."
    curl -L --progress-bar -o "$MODEL_FILE.tmp" "$MODEL_URL"
    echo "$MODEL_SHA256  $MODEL_FILE.tmp" | sha256sum -c - || {
        rm -f "$MODEL_FILE.tmp"
        echo "Model checksum failed!" >&2
        exit 1
    }
    mv "$MODEL_FILE.tmp" "$MODEL_FILE"
fi

for rc in /etc/bash.bashrc /etc/zsh/zshrc; do
    if [ -f "$rc" ] && ! grep -q "gcode-shell" "$rc"; then
        echo 'source /usr/share/gcode/gcode.bash  # gcode-shell' >> "$rc"
    fi
done
```

**4.2 RPM Spec (`packaging/rpm/gcode.spec`)**

```spec
Name:           gcode
Version:        1.0.0
Release:        1%{?dist}
Summary:        Natural language to Bash command generator
License:        MIT
URL:            https://github.com/yourname/gcode
Source0:        %{url}/archive/v%{version}/gcode-%{version}.tar.gz
BuildRequires:  rust cargo clang-devel openssl-devel
Requires:       jq curl

%description
gcode translates natural language to Bash commands using a local LLM
and recent shell history as context.

%prep
%setup -q

%build
cargo build --release --locked

%install
install -Dm755 target/release/gcode %{buildroot}%{_bindir}/gcode
install -Dm644 shell/gcode.bash %{buildroot}%{_datadir}/gcode/gcode.bash
install -Dm644 shell/gcode.zsh  %{buildroot}%{_datadir}/gcode/gcode.zsh

%files
%{_bindir}/gcode
%{_datadir}/gcode/

%changelog
* Mon Jan 01 2025 Mohammed <mohammed@example.com> - 1.0.0-1
- Initial package
```

**4.3 AUR PKGBUILD (`packaging/arch/PKGBUILD`)**

```bash
# Maintainer: Mohammed <mohammed@example.com>
pkgname=gcode
pkgver=1.0.0
pkgrel=1
pkgdesc="Natural language to Bash command generator"
arch=('x86_64' 'aarch64')
url="https://github.com/yourname/gcode"
license=('MIT')
depends=('gcc-libs' 'jq' 'curl')
makedepends=('rust' 'cargo' 'clang')
source=("$pkgname-$pkgver.tar.gz::$url/archive/v$pkgver.tar.gz")
sha256sums=('SKIP')

build() {
    cd "$pkgname-$pkgver"
    cargo build --release --locked
}

package() {
    cd "$pkgname-$pkgver"
    install -Dm755 target/release/gcode "$pkgdir/usr/bin/gcode"
    install -Dm644 shell/gcode.bash "$pkgdir/usr/share/gcode/gcode.bash"
    install -Dm644 shell/gcode.zsh  "$pkgdir/usr/share/gcode/gcode.zsh"
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
```

**4.4 AppImage (`packaging/appimage/AppRun`)**

```bash
#!/bin/bash
HERE="$(dirname "$(readlink -f "${0}")")"
export PATH="${HERE}/usr/bin:${PATH}"
export LD_LIBRARY_PATH="${HERE}/usr/lib:${LD_LIBRARY_PATH}"
exec "${HERE}/usr/bin/gcode" "$@"
```

**4.5 Homebrew Formula (`packaging/homebrew/gcode.rb`)**

```ruby
class Gcode < Formula
  desc "Natural language to Bash command generator"
  homepage "https://github.com/yourname/gcode"
  version "1.0.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/yourname/gcode/releases/download/v1.0.0/gcode-aarch64-apple-darwin.tar.gz"
      sha256 "abc..."
    else
      url "https://github.com/yourname/gcode/releases/download/v1.0.0/gcode-x86_64-apple-darwin.tar.gz"
      sha256 "def..."
    end
  end

  depends_on "jq"
  depends_on "curl"

  def install
    bin.install "gcode"
    (share/"gcode").install "gcode.bash", "gcode.zsh"
  end

  def caveats
    <<~EOS
      To enable shell integration, run:
        gcode --init
    EOS
  end

  test do
    assert_match "gcode", shell_output("#{bin}/gcode --version")
  end
end
```

#### معايير القبول
- [ ] `dpkg-buildpackage` ينتج `.deb` صالح
- [ ] `rpmbuild -ba` ينتج `.rpm` صالح
- [ ] `makepkg` ينتج حزمة AUR صالحة
- [ ] AppImage يعمل على Ubuntu 20.04, 22.04, 24.04
- [ ] `brew install` يعمل على macOS Intel + ARM
- [ ] Static binary يعمل على Alpine

### Phase 5 — التوزيع و CI/CD (أسبوع 7)

#### الأهداف
- [x] استضافة مستودع APT
- [x] استضافة مستودع RPM (COPR)
- [x] أتمتة إصدارات
- [x] GPG-sign للـ packages

#### المهام

**5.1 مستودع APT عبر GitHub Pages**

```
yourname.github.io/
└── gcode/
    ├── dists/stable/
    │   ├── Release
    │   ├── Release.gpg
    │   └── main/binary-amd64/Packages
    ├── pool/main/g/gcode/
    │   ├── gcode_1.0.0_amd64.deb
    │   └── gcode-model-kitty_1.0.0_all.deb
    └── gcode.gpg.key
```

**تثبيت المستخدم:**

```bash
curl -fsSL https://yourname.github.io/gcode/gcode.gpg.key \
  | sudo gpg --dearmor -o /usr/share/keyrings/gcode.gpg
echo "deb [signed-by=/usr/share/keyrings/gcode.gpg] \
  https://yourname.github.io/gcode stable main" \
  | sudo tee /etc/apt/sources.list.d/gcode.list
sudo apt update && sudo apt install gcode
```

**5.2 Release Workflow (`.github/workflows/release.yml`)**

```yaml
name: Release

on:
  push:
    tags: ['v*']

jobs:
  build-linux:
    strategy:
      matrix:
        target:
          - x86_64-unknown-linux-gnu
          - x86_64-unknown-linux-musl
          - aarch64-unknown-linux-gnu
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --release --target ${{ matrix.target }}
      - uses: actions/upload-artifact@v4
        with:
          name: gcode-${{ matrix.target }}
          path: target/${{ matrix.target }}/release/gcode

  build-macos:
    strategy:
      matrix:
        target:
          - x86_64-apple-darwin
          - aarch64-apple-darwin
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - run: cargo build --release --target ${{ matrix.target }}
      - uses: actions/upload-artifact@v4
        with:
          name: gcode-${{ matrix.target }}
          path: target/${{ matrix.target }}/release/gcode

  package-deb:
    needs: [build-linux]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
      - run: ./scripts/build-deb.sh
      - uses: actions/upload-artifact@v4
        with:
          name: deb-packages
          path: dist/*.deb

  create-release:
    needs: [package-deb]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
      - uses: softprops/action-gh-release@v1
        with:
          files: |
            **/*.deb
            **/*.rpm
            **/*.AppImage
            **/*.tar.gz
```

#### معايير القبول
- [ ] Tag push يُشغّل إصدارًا كاملًا
- [ ] كل المخرجات مرفوعة إلى GitHub Releases
- [ ] مستودع APT محدَّث تلقائيًا
- [ ] بناء COPR مُشغَّل
- [ ] Homebrew tap محدَّث
- [ ] كل الحزم موقَّعة GPG

### Phase 6 — الصقل والإطلاق (أسبوع 8)

#### الأهداف
- [x] التوثيق كامل
- [x] Man page
- [x] Shell completions
- [x] Demo GIF
- [x] v1.0.0 منشور

#### المهام

**6.1 Man Page (`docs/gcode.1`)**

```roff
.TH GCODE 1 "January 2025" "gcode 1.0.0" "User Commands"
.SH NAME
gcode \- natural language to Bash command generator
.SH SYNOPSIS
.B gcode
[\fIOPTIONS\fR] \fB-c\fR \fIREQUEST\fR
.SH DESCRIPTION
.B gcode
translates natural language requests into precise Bash commands using
a local LLM and recent shell history as context.
.SH OPTIONS
.TP
.BR \-c ", " \-\-command " " \fIREQUEST\fR
The natural language request.
.TP
.BR \-\-fix
Fix a failed command from the last exit code.
.TP
.BR \-\-complete " " \fIPARTIAL\fR
Complete a partial command.
```

**6.2 Shell Completions**

```bash
gcode --generate-completions bash > /usr/share/bash-completion/completions/gcode
gcode --generate-completions zsh > /usr/share/zsh/site-functions/_gcode
```

**6.3 Launch Checklist**
- [ ] README مصقول مع GIF
- [ ] موقع توثيق (GitHub Pages)
- [ ] نشر على r/linux, r/commandline, r/bash
- [ ] Hacker News (Show HN)
- [ ] Lobsters
- [ ] إضافة إلى awesome-selfhosted
- [ ] إضافة إلى awesome-cli-apps

---

## 11. إدارة النموذج

### 11.1 مواقع التخزين

| المنصة | المسار الافتراضي |
|---|---|
| Linux | `~/.local/share/gcode/models/` أو `/usr/share/gcode/models/` |
| macOS | `~/Library/Application Support/gcode/models/` |
| Override | `$GCODE_MODEL` env var |

### 11.2 استراتيجية التحميل

1. **عند أول تشغيل**: إذا لم يوجد نموذج:
   ```
   No model found. Download Kitty-bash-llm (398 MB)? [Y/n]
   ```
2. **التحقق**: SHA256 checksum مقابل `registry.toml`
3. **التخزين المؤقت**: في مجلد بيانات المستخدم
4. **الاستئناف**: دعم HTTP range requests
5. **Mirror**: fallback إلى URL ثانوي

### 11.3 تحديث النموذج

```bash
gcode --update-model           # تحديث الافتراضي
gcode --list-models            # عرض النماذج المتاحة
gcode --use-model bashgemma    # تبديل الافتراضي
```

---

## 12. استراتيجية الاختبار

### 12.1 اختبارات الوحدة

| الوحدة | تغطية مستهدفة | الأداة |
|---|---|---|
| `cli.rs` | 90% | `cargo test` |
| `safety/classifier.rs` | 100% | `cargo test` |
| `context/prompt.rs` | 90% | `insta` snapshots |
| `model/download.rs` | 80% | `mockito` |
| `history.rs` | 85% | `tempfile` |

### 12.2 اختبارات التكامل

```rust
// tests/integration/cli_test.rs
#[test]
fn test_simple_command_generation() {
    let out = Command::cargo_bin("gcode").unwrap()
        .args(&["-c", "list files", "--dry-run"])
        .output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ls"));
}

#[test]
fn test_critical_command_blocked() {
    let out = Command::cargo_bin("gcode").unwrap()
        .args(&["-c", "delete everything from root", "-y"])
        .output().unwrap();
    assert!(!out.status.success());
}
```

### 12.3 اختبارات الأداء

**الأهداف:**
- Cold start: < 500 ms
- Inference (64 tokens): < 1000 ms على 2-core CPU
- الذاكرة: < 1.5 GB RSS

### 12.4 مصفوفة الاختبار

| OS | Arch | الطريقة |
|---|---|---|
| Ubuntu 20.04 | x86_64 | GitHub Actions |
| Ubuntu 22.04 | x86_64 | GitHub Actions |
| Ubuntu 24.04 | x86_64 | GitHub Actions |
| Debian 12 | x86_64 | Docker |
| Fedora 40 | x86_64 | Docker |
| Arch Linux | x86_64 | Docker |
| Alpine 3.20 | x86_64 | Docker |
| macOS 13 | arm64 | GitHub Actions |
| macOS 14 | arm64 | GitHub Actions |
| macOS 12 | x86_64 | GitHub Actions |

---

## 13. CI/CD Pipeline

### 13.1 استراتيجية الفروع

```
main          ← محمي، يتطلب PR + CI pass
├── develop   ← فرع التكامل
├── feat/*    ← فروع الميزات
├── fix/*     ← فروع الإصلاح
└── release/* ← تحضير الإصدارات
```

### 13.2 متطلبات PR
- [ ] CI أخضر (test + lint + fmt)
- [ ] موافقة واحدة من المشرف
- [ ] لا انخفاض في تغطية الاختبارات
- [ ] إدخال CHANGELOG مضاف
- [ ] التوثيق محدَّث

### 13.3 إيقاع الإصدارات

| النوع | التكرار | ترقية الإصدار |
|---|---|---|
| Patch | حسب الحاجة | `1.0.x` |
| Minor | شهريًا | `1.x.0` |
| Major | سنويًا | `x.0.0` |

---

## 14. الأمان والخصوصية

### 14.1 مبادئ الخصوصية

1. **لا تتبع** — صفر تحليلات، صفر تقارير أعطال
2. **لا اتصالات شبكية** إلا لتحميل النموذج (opt-in)
3. **لا رفع للسجل** — كل السياق محلي
4. **مفتوح المصدر** — قابل للتدقيق

### 14.2 إجراءات الأمان

| التهديد | التخفيف |
|---|---|
| نموذج ضار | SHA256 verification, pinned URLs |
| Command injection | Grammar-constrained decoding |
| أوامر مدمّرة | Risk classifier + confirmation |
| Supply chain | `cargo-deny`, `cargo-audit` |
| تسريب السجل | File permissions `0600` |
| Path traversal | Validate all file paths |

### 14.3 سياسة الإفصاح

- البريد: `security@yourname.dev`
- الرد خلال 48 ساعة
- نافذة إفصاح 90 يومًا
- CVE للمشاكل الحرجة

---

## 15. التوثيق

| المستند | الجمهور | المحتوى |
|---|---|---|
| `README.md` | الجميع | نظرة عامة، تثبيت، بداية سريعة، GIF |
| `docs/INSTALL.md` | المستخدمون | أدلة التثبيت لكل منصة |
| `docs/USAGE.md` | المستخدمون | كل الأعلام، أمثلة، وصفات |
| `docs/MODELS.md` | المتقدمون | خيارات النموذج، GGUF مخصص |
| `docs/SAFETY.md` | المستخدمون | مستويات الخطر، التدفق |
| `docs/ARCHITECTURE.md` | المساهمون | التصميم الداخلي |
| `docs/CONTRIBUTING.md` | المساهمون | إعداد التطوير، PR process |
| `docs/TROUBLESHOOTING.md` | المستخدمون | مشاكل شائعة |
| `man gcode` | المستخدمون | Man page |

---

## 16. المخاطر والتخفيف

| # | المخاطرة | الاحتمال | التأثير | التخفيف |
|---|---|---|---|---|
| 1 | النموذج بطيء على CPU | متوسط | عالي | Q4_K_M quant, daemon mode, prompt caching |
| 2 | أوامر خطيرة مهلوسة | متوسط | حرج | Grammar + classifier + confirmation |
| 3 | Debian يرفض الحزمة | منخفض | متوسط | استضافة APT repo خاص |
| 4 | فشل تحميل النموذج | متوسط | متوسط | Mirrors متعددة، resume support |
| 5 | Shell hook يكسر prompt | منخفض | عالي | الحفاظ على `PROMPT_COMMAND` |
| 6 | Gatekeeper يحجب macOS | عالي | متوسط | Code signing + notarization |
| 7 | تبني منخفض | متوسط | متوسط | إطلاق على HN/Reddit، GIF |
| 8 | مشاكل ترخيص النموذج | منخفض | عالي | استخدام قاعدة Apache-2.0 |
| 9 | احتراق المشرف | متوسط | عالي | CONTRIBUTING واضح، تفويض |
| 10 | أداة منافسة | منخفض | متوسط | التركيز على local-first, safety |

---

## 17. مسار النمو المؤسسي

### 17.1 قمع التبني

```
┌─────────────────────────────────────────────────────────────┐
│  Stage 1: Individual Developer                              │
│  - تثبيت عبر curl أحد سطرين                                 │
│  - استخدام شخصي، لا تسجيل                                  │
│  - 0$ تكلفة                                                 │
└────────────────────────┬────────────────────────────────────┘
                         ↓ (بعد 1-3 أشهر)
┌─────────────────────────────────────────────────────────────┐
│  Stage 2: Team Adoption                                     │
│  - مشاركة الأوامر عبر git (snippets/)                       │
│  - نموذج مشترك على server داخلي                             │
│  - Self-hosted، لا تكلفة                                    │
└────────────────────────┬────────────────────────────────────┘
                         ↓ (بعد 3-6 أشهر)
┌─────────────────────────────────────────────────────────────┐
│  Stage 3: Enterprise Deployment                             │
│  - SSO/RBAC عبر Okta/Azure AD                               │
│  - Compliance mode (audit logs, policies)                   │
│  - دعم تجاري (SLA, support)                                 │
│  - تكلفة: $X/user/month                                     │
└────────────────────────┬────────────────────────────────────┘
                         ↓ (بعد سنة)
┌─────────────────────────────────────────────────────────────┐
│  Stage 4: Platform / Ecosystem                              │
│  - API عامة للمطورين                                        │
│  - Marketplace للنماذج والإضافات                            │
│  - تكاملات عميقة (AWS, GCP, K8s, Datadog)                  │
│  - استحواذ محتمل من شركة كبرى                               │
└─────────────────────────────────────────────────────────────┘
```

### 17.2 المتطلبات لتبنّي الشركات

| المتطلب | الحل في gcode |
|---|---|
| **SSO** | تكامل Okta/Azure AD/Google |
| **RBAC** | سياسات حسب الدور |
| **Audit logs** | JSONL + SIEM integration |
| **Compliance** | SOC2, ISO 27001, HIPAA modes |
| **On-premise** | نشر ذاتي بالكامل |
| **Data residency** | كل شيء محلي |
| **Model control** | نماذج معتمدة فقط |
| **Air-gapped** | يعمل بدون إنترنت |
| **Central config** | إدارة عبر MDM/Ansible |
| **SIEM integration** | Webhooks + syslog |
| **Cost control** | حدود استخدام لكل فريق |
| **Custom models** | نماذج مخصصة للشركة |

### 17.3 الفريق المطلوب للنمو

| المرحلة | الفريق | الأدوار |
|---|---|---|
| **v1.0** | 1 مطور | Founder/Maintainer |
| **v1.5** | 2-3 مطورين | + Community manager |
| **v2.0** | 5-7 مطورين | + Security engineer، + DevOps |
| **v3.0** | 10-15 مطور | + Sales، + Support، + Docs |
| **v4.0** | 20+ | منظمة كاملة |

### 17.4 فرص الاستحواذ

| الشركة | السبب |
|---|---|
| **HashiCorp** | تكامل مع Terraform/Vault |
| **GitLab** | ميزة في CI/CD |
| **Datadog** | تكامل مع monitoring |
| **Cloudflare** | ميزة في Workers/Wrangler |
| **Microsoft** | GitHub Copilot CLI |
| **Amazon** | AWS CLI integration |
| **Red Hat** | RHEL default tool |
| **Canonical** | Ubuntu default tool |

---

## 18. نموذج العمل

| الفئة | السعر | الميزات |
|---|---|---|
| **Community** | مجاني | كل الميزات الأساسية، استخدام فردي |
| **Team** | $10/user/month | Knowledge base مشترك، snippets، دعم بريد |
| **Business** | $25/user/month | SSO، RBAC، audit logs، سياسات |
| **Enterprise** | مخصص | on-premise، دعم 24/7، تدريب مخصص، SLA |
| **Marketplace** | 30% عمولة | بيع النماذج والإضافات |

**المصادر البديلة للإيراد:**
- دعم تجاري (Support contracts)
- استشارات وتدريب
- نماذج مخصصة (custom fine-tunes)
- تراخيص OEM للشركات الكبرى

---

## 19. مؤشرات النجاح

### 19.1 الإطلاق (شهر 1)

| المؤشر | الهدف |
|---|---|
| GitHub stars | 500+ |
| Homebrew installs | 200+ |
| APT installs | 300+ |
| Issues opened | 20+ |
| PRs من المجتمع | 5+ |

### 19.2 v1.1 (شهر 3)

| المؤشر | الهدف |
|---|---|
| GitHub stars | 2,000+ |
| Monthly active users | 1,000+ |
| Fish shell support | ✅ |
| Custom model support | ✅ |

### 19.3 v1.5 (شهر 6)

| المؤشر | الهدف |
|---|---|
| GitHub stars | 5,000+ |
| Contributors | 15+ |
| Optional cloud LLM | ✅ |
| Conversation mode | ✅ |

### 19.4 السنة 1

| المؤشر | الهدف |
|---|---|
| GitHub Stars | 10,000+ |
| Contributors | 50+ |
| Monthly Active Users | 50,000+ |
| Enterprise Customers | 5+ |
| ARR | $500K+ |

### 19.5 السنة 2

| المؤشر | الهدف |
|---|---|
| GitHub Stars | 30,000+ |
| Enterprise Customers | 50+ |
| ARR | $5M+ |
| Model Marketplace | 100+ نموذج |
| التبني المؤسسي | 10% من Fortune 500 |

### 19.6 السنة 3

| المؤشر | الهدف |
|---|---|
| المستخدمون | 1M+ |
| ARR | $25M+ |
| التقييم | $200M+ |
| فرص استحواذ | 3+ |

---

## 20. خارطة الطريق المستقبلية

### v1.1 — تغطية القشور
- [ ] Fish shell support
- [ ] Nushell support
- [ ] Better completion (tab-triggered)

### v1.2 — مرونة النموذج
- [ ] Custom GGUF support
- [ ] Multi-model switching
- [ ] Model auto-update

### v1.3 — الأداء
- [ ] Daemon mode (`gcode --serve`)
- [ ] Prompt caching
- [ ] GPU acceleration (CUDA, Metal, Vulkan)

### v1.4 — الذكاء
- [ ] Multi-step command generation
- [ ] Command explanation mode
- [ ] Learn from user edits

### v1.5 — التكاملات
- [ ] Optional cloud LLM
- [ ] VS Code extension
- [ ] tmux plugin
- [ ] Neovim plugin
- [ ] Conversational mode
- [ ] Sandbox preview
- [ ] Rollback engine

### v2.0 — Enterprise
- [ ] SSO (Okta, Azure AD, Google)
- [ ] RBAC
- [ ] Compliance mode (SOC2, ISO 27001)
- [ ] Audit logs + SIEM integration
- [ ] Policy engine (Rego-like)
- [ ] Domain-specific models
- [ ] Cost estimation
- [ ] Incident response mode
- [ ] Voice input
- [ ] Screen context
- [ ] Time-travel shell
- [ ] Shell autopsy

### v2.5 — المنصة
- [ ] WASM plugin system
- [ ] Platform API
- [ ] Marketplace
- [ ] Web UI

### v3.0+ — ما بعد Bash
- [ ] Windows (PowerShell)
- [ ] Zsh completion integration
- [ ] Multi-language (Python, Docker, kubectl)
- [ ] Federated learning

---

## 21. نصائح استراتيجية

### 21.1 اجعل التثبيت أولوية قصوى

**القاعدة الذهبية:** كل 10 ثوانٍ إضافية في التثبيت = 50% فقدان للمستخدمين.

- **لا تتجاوز 30 ثانية** من curl إلى أول أمر ناجح
- **لا تطلب sudo** إلا إذا كان ضروريًا
- **لا تطلب تسجيل** أبدًا للاستخدام الأساسي
- **حمّل النموذج في الخلفية** أثناء الاستخدام الأول

### 21.2 ابنِ للأفراد أولًا، للشركات لاحقًا

- **v1.0**: تجربة فردية ممتازة
- **v1.5**: تعاون فريقي
- **v2.0**: ميزات مؤسسية

**لا تبدأ بالـ Enterprise** — ستفقد المطورين الأفراد.

### 21.3 المجتمع هو الوقود

- **GitHub Discussions** بدل Issues للأفكار
- **Discord/Slack** للمناقشات السريعة
- **Contributor guide** واضح جدًا
- **Good first issues** دائمًا متوفرة
- **Monthly community call**
- **Ambassador program** للدول المختلفة

### 21.4 التسويق الذكي

- **Demo GIF** في README (أهم شيء)
- **Show HN** عند الإطلاق
- **r/linux, r/bash, r/devops** posts
- **YouTube demos**
- **Podcast interviews**
- **Conference talks** (FOSDEM, KubeCon)

### 21.5 حافظ على البساطة

- **لا تُضف ميزة** إلا إذا كانت 80% من المستخدمين سيستخدمونها
- **لا تعقّد الإعدادات** — القيم الافتراضية يجب أن تكون الأفضل
- **لا تجبر على الإنترنت** — الأداة يجب أن تعمل offline دائمًا
- **لا تجمع بيانات** — الخصوصية خط أحمر

### 21.6 الطريق للاستحواذ

لجذب انتباه شركة كبرى:

1. **بناء مستخدمين مخلصين** (10K+ stars)
2. **ميزات لا يمكن تقليدها بسهولة** (النموذج، التكامل)
3. **حضور قوي في المؤتمرات**
4. **أرقام نمو واضحة** (MAU, retention)
5. **شراكات استراتيجية** (مع AWS, GitLab, إلخ)
6. **فريق قوي** (مستشارون من الصناعة)

---

## 📅 ملخص الجدول الزمني

```
Week 1   ████████  Phase 0: الأساس
Week 2   ████████  Phase 1: المحرك الأساسي (جزء 1)
Week 3   ████████  Phase 1: المحرك الأساسي (جزء 2)
Week 4   ████████  Phase 2: تكامل القشرة
Week 5   ████████  Phase 3: طبقة الأمان
Week 6   ████████  Phase 4: التغليف
Week 7   ████████  Phase 5: التوزيع و CI/CD
Week 8   ████████  Phase 6: الصقل والإطلاق v1.0
```

**الإجمالي: 8 أسابيع (~160 ساعة بدوام جزئي)**

---

## ✅ قائمة التحقق النهائية قبل v1.0

- [ ] كل معايير القبول للمراحل 0–6 محققة
- [ ] تغطية الاختبارات > 80%
- [ ] Benchmarks داخل الأهداف
- [ ] التوثيق كامل
- [ ] Man page مكتوب
- [ ] Shell completions مُولَّدة
- [ ] Demo GIF مُسجَّل
- [ ] الحزم مبنية وموقَّعة
- [ ] مستودعات APT/COPR/Homebrew حية
- [ ] README مصقول
- [ ] CHANGELOG مكتوب
- [ ] LICENSE مُودَع
- [ ] SECURITY.md مُودَع
- [ ] CONTRIBUTING.md مُودَع
- [ ] GitHub Release منشور
- [ ] منشورات الإطلاق جاهزة

---

## 💎 الخلاصة

`gcode` لديها فرصة حقيقية لتصبح **الأداة المعيارية** لتحويل اللغة الطبيعية إلى أوامر في الطرفية، ثم تتوسّع لتصبح **منصة كاملة** لتفاعل المطورين مع الأنظمة.

**المفتاح:**
1. **ابدأ بسيطًا** — تثبيت بأمر واحد، تجربة فورية
2. **ابنِ للأفراد** — تجربة شخصية ممتازة
3. **توسّع للفرق** — مشاركة ومعرفة مشتركة
4. **استهدف الشركات** — SSO، RBAC، Compliance
5. **ابنِ منصة** — API، Marketplace، Plugins

**بعد 3 سنوات، `gcode` يمكن أن تكون:**
- 🔥 أداة يستخدمها 1M+ مطور
- 💰 شركة بقيمة $200M+
- 🏢 معيارًا في Fortune 500
- 🌍 مشروعًا مفتوح المصدر يحبه المجتمع

**الرحلة تبدأ بسطر واحد:**

```bash
curl -fsSL https://get.gcode.dev | sh
```

---

**الترخيص:** MIT (مع نموذج تجاري للميزات المؤسسية)
**المشرف:** Mohammed <mohammed@example.com>
**المستودع:** https://github.com/yourname/gcode
**الموقع:** https://gcode.dev

*آخر تحديث: 2025*
