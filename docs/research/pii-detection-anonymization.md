# PII Detection & Anonymization for Rust-Based Systems - Research Report

Date: 2026-03-16

---

## 1. Rust-Native PII/NER Libraries

### 1.1 worka-ai/pii (Recommended)

- **Repo**: https://github.com/worka-ai/pii
- **Crate**: `pii` on crates.io
- **License**: MIT/Apache-2.0
- **Architecture**: Presidio-inspired, pure Rust
- **Detection methods**: Regex, validator, dictionary, and NER-backed recognizers
- **Built-in entity types** (no NER needed):
  - Email, Phone, IpAddress (v4/v6), CreditCard, IBAN, SSN, ITIN
  - TaxId, Passport, DriverLicense, BankAccount, RoutingNumber
  - CryptoAddress, MacAddress, UUID, VIN, IMEI, URL, Domain, Hostname
- **NER-backed types** (with `candle-ner` feature):
  - Person, Location, Organization
- **Anonymization operators**: Redact, Mask, Replace, Hash
- **Key strengths**:
  - Deterministic detection with stable byte offsets
  - Capability-aware pipeline (tokenization, lemma, POS, NER)
  - Fully customizable: custom recognizers, NLP engines, context enhancers
  - CPU-only, no GPU required
  - Optional Candle-based NER via feature flag
- **Limitation**: No built-in Chinese PII patterns (needs custom recognizers)

```rust
// Example: Custom recognizer for Chinese ID
let id_recognizer = RegexRecognizer::new(
    "regex_chinese_id",
    EntityType::Custom("ChineseIdCard".to_string()),
    r"\b[1-9]\d{5}(?:19|20)\d{2}(?:0[1-9]|1[0-2])(?:0[1-9]|[12]\d|3[01])\d{3}[\dXx]\b",
    0.85,
    "chinese_id_card",
).unwrap();
```

### 1.2 pii-shield (Reference Architecture)

- **Repo**: https://github.com/swissprismia/pii-shield
- **Type**: Tauri desktop app (Rust frontend + Python sidecar)
- **Architecture**: Rust clipboard monitor + Presidio/spaCy via Python sidecar
- **Key insight**: Demonstrates the exact data flow we need:
  ```
  Copy text → Detect PII → Tokenize ([FirstName1], [Email1]) → Paste to AI
  Copy AI response → Detect tokens → Restore original PII → Paste back
  ```
- **Token vault**: In-memory bidirectional mapping for round-trip anonymization
- **Relevance**: Proves the pattern works; we can replicate in pure Rust

### 1.3 rust-bert

- **Repo**: https://github.com/guillaume-be/rust-bert
- **Stars**: 2,600+
- **Provides**: NER pipeline via Rust bindings to libtorch
- **Models**: BERT, DistilBERT, etc. for token classification
- **Drawback**: Requires libtorch (large dependency, ~2GB), not ideal for lightweight deployment
- **Use case**: If you need high-accuracy NER and can tolerate the dependency

### 1.4 chinese-ner-rs

- **Repo**: https://github.com/messense/chinese-ner-rs
- **Type**: CRF-based Chinese NER in Rust
- **Stars**: 14
- **Relevance**: Purpose-built for Chinese NER, lightweight
- **Limitation**: Small project, may need maintenance

### 1.5 textminer-rs

- **Repo**: https://github.com/k5602/textminer-rs
- **Type**: BERT-based NER microservice in Rust (Actix Web)
- **Relevance**: Shows how to build a PII redaction service with BERT + Rust

---

## 2. Microsoft Presidio

### 2.1 Architecture

Presidio consists of two main services:

```
┌─────────────────────┐     ┌──────────────────────┐
│  Presidio Analyzer   │     │  Presidio Anonymizer  │
│                      │     │                       │
│  - PatternRecognizer │     │  - Replace            │
│  - SpacyRecognizer   │────▶│  - Redact             │
│  - TransformersRec.  │     │  - Mask               │
│  - Custom Recognizers│     │  - Hash               │
│                      │     │  - Encrypt/Decrypt    │
└─────────────────────┘     └──────────────────────┘
```

### 2.2 Integration Options for Rust

**Option A: REST API Sidecar (Recommended for quick start)**
```
Rust App ──HTTP──▶ Presidio Analyzer (Docker) ──▶ Presidio Anonymizer (Docker)
```
- Deploy as Docker containers alongside Rust service
- REST API: `POST /analyze` and `POST /anonymize`
- Latency: ~10-50ms per request (local Docker)
- Supports custom recognizers via configuration

**Option B: Python Sidecar (as pii-shield does)**
```
Rust App ──stdin/stdout JSON──▶ Python process (Presidio + spaCy)
```
- Bundle Python with PyInstaller
- Communicate via JSON over stdin/stdout
- Lower latency than HTTP, but requires Python runtime

**Option C: Pure Rust reimplementation (Best long-term)**
- Use `worka-ai/pii` crate (already Presidio-inspired)
- Add custom recognizers for Chinese PII
- Add ONNX-based NER for Person/Location/Organization

### 2.3 Presidio Reversible Anonymization

Presidio supports `Decrypt` operator for reversible anonymization:
```python
# Encrypt with AES
OperatorConfig("encrypt", {"key": "WmZq4t7w!z%C&F)J"})
# Decrypt to restore
DeanonymizeConfig("decrypt", {"key": "WmZq4t7w!z%C&F)J"})
```

---

## 3. Regex-Based PII Detection Patterns

### 3.1 Chinese-Specific PII Patterns

```rust
// 中国身份证号 (18位)
const CHINESE_ID_CARD: &str =
    r"[1-9]\d{5}(?:19|20)\d{2}(?:0[1-9]|1[0-2])(?:0[1-9]|[12]\d|3[01])\d{3}[\dXx]";

// 中国手机号 (11位, 1开头)
const CHINESE_MOBILE: &str =
    r"(?:\+?86[-\s]?)?1[3-9]\d{9}";

// 中国银行卡号 (16-19位)
const CHINESE_BANK_CARD: &str =
    r"(?:62|4|5|3)\d{14,17}";

// 中国固定电话
const CHINESE_LANDLINE: &str =
    r"(?:0\d{2,3}[-\s]?)?\d{7,8}";

// 中国邮政编码
const CHINESE_POSTAL: &str =
    r"[1-9]\d{5}(?!\d)";

// 中国护照号
const CHINESE_PASSPORT: &str =
    r"(?:E|G|D|S|P|H|M)\d{8}";

// 中国统一社会信用代码 (18位)
const CHINESE_USCC: &str =
    r"[0-9A-HJ-NPQRTUWXY]{2}\d{6}[0-9A-HJ-NPQRTUWXY]{10}";

// 中国车牌号
const CHINESE_LICENSE_PLATE: &str =
    r"[京津沪渝冀豫云辽黑湘皖鲁新苏浙赣鄂桂甘晋蒙陕吉闽贵粤川青藏琼宁][A-Z][A-HJ-NP-Z0-9]{4,5}[A-HJ-NP-Z0-9挂学警港澳]";
```

### 3.2 Universal PII Patterns

```rust
// Email
const EMAIL: &str = r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}";

// International phone (E.164)
const INTL_PHONE: &str = r"\+[1-9]\d{6,14}";

// Credit card (Luhn-validatable)
const CREDIT_CARD: &str = r"(?:4\d{12}(?:\d{3})?|5[1-5]\d{14}|3[47]\d{13}|6(?:011|5\d{2})\d{12})";

// IPv4
const IPV4: &str = r"(?:(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d\d?)";

// US SSN
const US_SSN: &str = r"(?!000|666|9\d{2})\d{3}-(?!00)\d{2}-(?!0000)\d{4}";
```

### 3.3 Chinese Name Detection (Dictionary + Heuristic)

Chinese names cannot be reliably detected by regex alone. Approaches:
1. **Common surname dictionary**: 百家姓 (top 500 surnames) + 2-3 character given name pattern
2. **NER model**: Best accuracy for names in context
3. **Hybrid**: Surname dictionary + NER confirmation

```rust
// Heuristic: Common surname + 1-2 characters
// This has HIGH false positive rate - use only as pre-filter for NER
const CHINESE_NAME_HEURISTIC: &str =
    r"[赵钱孙李周吴郑王冯陈褚卫蒋沈韩杨朱秦尤许何吕施张孔曹严华金魏陶姜戚谢邹喻柏水窦章云苏潘葛奚范彭郎鲁韦昌马苗凤花方俞任袁柳酆鲍史唐费廉岑薛雷贺倪汤滕殷罗毕郝邬安常乐于时傅皮卞齐康伍余元卜顾孟平黄和穆萧尹姚邵湛汪祁毛禹狄米贝明臧计伏成戴谈宋茅庞熊纪舒屈项祝董梁杜阮蓝闵席季麻强贾路娄危江童颜郭梅盛林刁钟徐邱骆高夏蔡田樊胡凌霍虞万支柯昝管卢莫经房裘缪干解应宗丁宣贲邓郁单杭洪包诸左石崔吉钮龚程嵇邢滑裴陆荣翁荀羊於惠甄曲家封芮羿储靳汲邴糜松井段富巫乌焦巴弓牧隗山谷车侯宓蓬全郗班仰秋仲伊宫宁仇栾暴甘钭厉戎祖武符刘景詹束龙叶幸司韶郜黎蓟薄印宿白怀蒲邰从鄂索咸籍赖卓蔺屠蒙池乔阴郁胥能苍双闻莘党翟谭贡劳逄姬申扶堵冉宰郦雍却璩桑桂濮牛寿通边扈燕冀郏浦尚农温别庄晏柴瞿阎充慕连茹习宦艾鱼容向古易慎戈廖庾终暨居衡步都耿满弘匡国文寇广禄阙东欧殳沃利蔚越夔隆师巩厍聂晁勾敖融冷訾辛阚那简饶空曾毋沙乜养鞠须丰巢关蒯相查后荆红游竺权逯盖益桓公][\\u4e00-\\u9fff]{1,2}";
```

---

## 4. ONNX-Based NER Models

### 4.1 Rust ONNX Runtime (`ort` crate)

The `ort` crate (https://crates.io/crates/ort) provides Rust bindings for ONNX Runtime:
- Version 2.0+ with clean API
- Supports CPU, CUDA, TensorRT, CoreML, DirectML
- Token classification (NER) inference is straightforward

```rust
use ort::{Session, Value};

let session = Session::builder()?
    .with_optimization_level(GraphOptimizationLevel::Level3)?
    .commit_from_file("ner_model.onnx")?;

let input_ids = Value::from_array(/* tokenized input */)?;
let attention_mask = Value::from_array(/* attention mask */)?;

let outputs = session.run(ort::inputs![input_ids, attention_mask]?)?;
let logits = outputs[0].extract_tensor::<f32>()?;
// argmax over logits to get NER labels
```

### 4.2 Recommended Models for Chinese + English NER

| Model | Size | Languages | Entities | Source |
|-------|------|-----------|----------|--------|
| `dslim/bert-base-NER` | ~440MB | English | PER, LOC, ORG, MISC | HuggingFace |
| `bert-base-chinese` fine-tuned on CLUENER | ~400MB | Chinese | PER, LOC, ORG, etc. | HuggingFace |
| `xlm-roberta-base` fine-tuned NER | ~1.1GB | Multilingual | PER, LOC, ORG | HuggingFace |
| `Davlan/distilbert-base-multilingual-cased-ner-hrl` | ~270MB | Multi (incl. Chinese) | PER, LOC, ORG | HuggingFace |

**Recommended for our use case**: `Davlan/distilbert-base-multilingual-cased-ner-hrl`
- Smallest multilingual model (~270MB)
- Supports Chinese + English
- Can be exported to ONNX via `optimum`

### 4.3 ONNX Export

```bash
# Export HuggingFace model to ONNX
pip install optimum[exporters]
optimum-cli export onnx \
  --model Davlan/distilbert-base-multilingual-cased-ner-hrl \
  --task token-classification \
  ./ner-onnx/

# Quantize for smaller size + faster CPU inference
optimum-cli onnxruntime quantize \
  --onnx_model ./ner-onnx/ \
  --avx512 \
  -o ./ner-onnx-quantized/
```

After quantization: ~70MB (INT8), suitable for local deployment.

### 4.4 Alternative: Candle (Rust-native ML)

The `candle` crate by HuggingFace provides pure Rust ML inference:
- No external runtime dependency
- Supports BERT, DistilBERT architectures
- The `worka-ai/pii` crate already has `candle-ner` feature flag
- Smaller binary than ONNX Runtime

---

## 5. Chinese-Specific PII Strategy

### 5.1 Detection Layers (Hybrid Approach)

```
Layer 1: Regex (Fast, High Precision for structured data)
├── 身份证号 (ID card)         → regex + checksum validation
├── 手机号 (Mobile phone)      → regex + carrier prefix validation
├── 银行卡号 (Bank card)       → regex + Luhn check
├── 护照号 (Passport)          → regex
├── 统一社会信用代码 (USCC)     → regex + checksum
└── 车牌号 (License plate)     → regex

Layer 2: NER Model (For unstructured entities)
├── 姓名 (Person name)         → BERT/DistilBERT NER
├── 地址 (Address)             → BERT/DistilBERT NER
├── 组织机构 (Organization)    → BERT/DistilBERT NER
└── 地名 (Location)            → BERT/DistilBERT NER

Layer 3: Context Enhancement (Boost confidence)
├── "我叫" / "姓名:" prefix    → boost Person confidence
├── "住在" / "地址:" prefix    → boost Location confidence
├── "身份证号:" prefix         → boost ID confidence
└── Surrounding entity co-occurrence → boost all
```

### 5.2 Chinese ID Card Validation (with checksum)

```rust
fn validate_chinese_id(id: &str) -> bool {
    if id.len() != 18 { return false; }
    let weights = [7, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2];
    let check_codes = ['1', '0', 'X', '9', '8', '7', '6', '5', '4', '3', '2'];

    let chars: Vec<char> = id.chars().collect();
    let sum: u32 = chars[..17].iter()
        .zip(weights.iter())
        .filter_map(|(c, w)| c.to_digit(10).map(|d| d * (*w as u32)))
        .sum();

    let expected = check_codes[(sum % 11) as usize];
    let actual = chars[17].to_ascii_uppercase();
    expected == actual
}
```

---

## 6. Token Mapping / Reversible Anonymization

### 6.1 Architecture

```
┌──────────────────────────────────────────────────────────┐
│                    PII Proxy Layer                        │
│                                                          │
│  User Input                                              │
│  "张三的身份证号是110101199001011234，手机号13812345678"    │
│       │                                                  │
│       ▼                                                  │
│  ┌─────────────┐                                         │
│  │ PII Detector │ (Regex + NER)                          │
│  └─────┬───────┘                                         │
│        │ Detected: [Person:"张三", IdCard:"110...", Phone:"138..."] │
│        ▼                                                  │
│  ┌──────────────┐    ┌─────────────────────┐             │
│  │ Token Mapper  │───▶│ Token Vault (AES)   │             │
│  └──────┬───────┘    │ [PERSON_a1b2] → 张三 │             │
│         │            │ [IDCARD_c3d4] → 110..│             │
│         │            │ [PHONE_e5f6] → 138.. │             │
│         ▼            └─────────────────────┘             │
│  Anonymized Text                                         │
│  "[PERSON_a1b2]的身份证号是[IDCARD_c3d4]，手机号[PHONE_e5f6]" │
│         │                                                │
│         ▼                                                │
│  ┌──────────────┐                                        │
│  │  Cloud LLM    │                                       │
│  └──────┬───────┘                                        │
│         │                                                │
│         ▼                                                │
│  LLM Response                                            │
│  "[PERSON_a1b2]的身份证号[IDCARD_c3d4]属于北京市..."       │
│         │                                                │
│         ▼                                                │
│  ┌──────────────┐                                        │
│  │ Token Restorer│ (lookup Token Vault)                   │
│  └──────┬───────┘                                        │
│         │                                                │
│         ▼                                                │
│  Restored Response                                       │
│  "张三的身份证号110101199001011234属于北京市..."            │
└──────────────────────────────────────────────────────────┘
```

### 6.2 Token Vault Design

```rust
use std::collections::HashMap;
use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, NewAead};
use uuid::Uuid;

/// Bidirectional mapping between PII values and tokens
struct TokenVault {
    /// token_id → encrypted PII value
    token_to_pii: HashMap<String, Vec<u8>>,
    /// hash(pii_value) → token_id (for dedup)
    pii_hash_to_token: HashMap<[u8; 32], String>,
    /// AES-256-GCM key for encrypting stored values
    cipher: Aes256Gcm,
    /// Counter for deterministic nonces within a session
    nonce_counter: u64,
}

impl TokenVault {
    fn new(key: &[u8; 32]) -> Self {
        let cipher = Aes256Gcm::new(Key::from_slice(key));
        Self {
            token_to_pii: HashMap::new(),
            pii_hash_to_token: HashMap::new(),
            cipher,
            nonce_counter: 0,
        }
    }

    /// Store PII and return a placeholder token
    fn tokenize(&mut self, entity_type: &str, pii_value: &str) -> String {
        // Dedup: return existing token if same value seen before
        let hash = sha256(pii_value);
        if let Some(token) = self.pii_hash_to_token.get(&hash) {
            return token.clone();
        }

        // Generate token: [TYPE_short-uuid]
        let short_id = &Uuid::new_v4().to_string()[..8];
        let token = format!("[{}_{short_id}]", entity_type.to_uppercase());

        // Encrypt and store
        self.nonce_counter += 1;
        let nonce_bytes = self.nonce_counter.to_le_bytes();
        let mut nonce_arr = [0u8; 12];
        nonce_arr[..8].copy_from_slice(&nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_arr);

        let encrypted = self.cipher.encrypt(nonce, pii_value.as_bytes()).unwrap();
        self.token_to_pii.insert(token.clone(), encrypted);
        self.pii_hash_to_token.insert(hash, token.clone());

        token
    }

    /// Restore original PII from token
    fn detokenize(&self, token: &str) -> Option<String> {
        self.token_to_pii.get(token).and_then(|encrypted| {
            // Need to try nonces (or store nonce alongside ciphertext)
            // Simplified: in production, store (nonce, ciphertext) together
            None // placeholder
        })
    }
}
```

### 6.3 Key Design Decisions

1. **Token format**: `[TYPE_shortid]` - LLMs handle bracketed placeholders well
2. **Deduplication**: Same PII value always maps to same token within a session
3. **Encryption at rest**: Even in-memory vault encrypts values (defense in depth)
4. **Session-scoped**: Vault lives for one conversation/request, then destroyed
5. **Deterministic tokens**: Same input → same token (important for multi-turn conversations)

### 6.4 Security Considerations

- AES-256-GCM for encryption of stored PII values
- Session key derived from secure random source, never persisted
- Token vault cleared after response is sent
- For multi-turn: vault persists for conversation duration, keyed per user session
- Audit log: record token creation/restoration events (without PII values)
- Rate limiting on detokenize to prevent brute-force

---

## 7. Recommended Architecture

### 7.1 Hybrid Detection Pipeline

```
┌─────────────────────────────────────────────────┐
│              PII Detection Pipeline              │
│                                                  │
│  Input Text                                      │
│      │                                           │
│      ├──▶ [Regex Engine] ──────────────────┐     │
│      │    Fast, structured PII             │     │
│      │    (ID, phone, email, card, etc.)   │     │
│      │                                     │     │
│      ├──▶ [ONNX NER Engine] ──────────┐   │     │
│      │    Unstructured PII            │   │     │
│      │    (names, addresses, orgs)    │   │     │
│      │                                │   │     │
│      └──▶ [Context Enhancer] ─────┐   │   │     │
│           Boost/suppress scores   │   │   │     │
│                                   ▼   ▼   ▼     │
│                              ┌─────────────┐    │
│                              │  Merger &    │    │
│                              │  Dedup       │    │
│                              │  (by span)   │    │
│                              └──────┬──────┘    │
│                                     │            │
│                                     ▼            │
│                              Detected Entities   │
└─────────────────────────────────────────────────┘
```

### 7.2 Crate Dependencies

```toml
[dependencies]
# Core PII detection (Presidio-like, pure Rust)
pii = { version = "0.x", features = ["candle-ner"] }

# ONNX Runtime for NER models
ort = { version = "2", features = ["load-dynamic"] }

# Tokenizer for BERT models
tokenizers = "0.20"

# Encryption for token vault
aes-gcm = "0.10"
sha2 = "0.10"

# Regex engine
regex = "1"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# UUID for token generation
uuid = { version = "1", features = ["v4"] }
```

### 7.3 Complete Data Flow

```
User Input
    │
    ▼
┌─────────────────────┐
│ 1. Language Detect   │  (detect zh/en for model routing)
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ 2. Regex Scan       │  (structured PII: ID, phone, email, card)
│    ~1ms latency     │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ 3. NER Inference    │  (unstructured PII: names, addresses)
│    ~20-50ms (ONNX)  │  (DistilBERT multilingual, quantized)
│    ~50-100ms(Candle) │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ 4. Merge & Score    │  (deduplicate overlapping spans, threshold)
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ 5. Tokenize         │  (replace PII with [TYPE_id] tokens)
│    Store in Vault   │  (encrypted bidirectional map)
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ 6. Send to Cloud LLM│  (anonymized text only)
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ 7. Receive Response  │  (contains [TYPE_id] tokens)
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ 8. Detokenize       │  (restore PII from vault)
└─────────┬───────────┘
          │
          ▼
Final Response (with original PII restored)
```

### 7.4 Performance Budget

| Step | Latency | Notes |
|------|---------|-------|
| Language detection | <1ms | Simple heuristic (Unicode range) |
| Regex scan | 1-5ms | All patterns in parallel via RegexSet |
| NER inference | 20-50ms | Quantized DistilBERT on CPU |
| Merge & score | <1ms | In-memory span operations |
| Tokenize | <1ms | HashMap + AES encrypt |
| Detokenize | <1ms | HashMap lookup + AES decrypt |
| **Total overhead** | **~25-60ms** | Acceptable for LLM proxy |

---

## 8. Implementation Recommendations

### Phase 1: Regex-Only (1-2 weeks)
- Implement regex recognizers for all structured Chinese + English PII
- Build token vault with AES-256-GCM encryption
- Build anonymize/deanonymize pipeline
- Integrate as middleware in the LLM proxy

### Phase 2: Add NER (2-3 weeks)
- Export DistilBERT multilingual NER to ONNX + quantize
- Integrate via `ort` crate for inference
- Or use `worka-ai/pii` with `candle-ner` feature
- Add context enhancers for Chinese patterns

### Phase 3: Production Hardening (1-2 weeks)
- Audit logging (without PII values)
- Configurable entity types and thresholds per tenant
- Multi-turn conversation vault management
- Performance benchmarking and optimization
- Fallback: if NER model fails, degrade to regex-only

### Alternative: Presidio Sidecar (if time-constrained)
- Deploy Presidio as Docker sidecar
- Call via REST API from Rust
- Add custom Chinese recognizers to Presidio config
- Trade-off: adds Python/Docker dependency but gets production-grade NLP immediately
