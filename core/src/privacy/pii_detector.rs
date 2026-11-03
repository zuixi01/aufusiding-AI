//! 多层 PII 检测引擎
//!
//! 支持正则匹配、校验算法、字典匹配和上下文检测。

use regex::Regex;
use std::collections::HashSet;
use std::sync::Arc;

use super::context_leak_detector::ContextLeakDetector;
use super::sensitive_dictionary::SensitiveDictionary;

/// PII 实体类型
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PiiType {
    ChinesePhone,
    InternationalPhone,
    Email,
    ChineseIdCard,
    BankCard,
    ChineseName,
    IpAddress,
    Url,
    ApiKey,
    Password,
    LicensePlate,
    Passport,
    SocialCredit,
    GeoCoordinate,
    PhysicalAddress,
    MacAddress,
    Imei,
    CryptoAddress,
    CloudCredential,
    PrivateKey,
    DbConnectionString,
    JwtToken,
    Vin,
    Ipv6Address,
    Iban,
    Custom(String),
}

impl std::fmt::Display for PiiType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChinesePhone => write!(f, "PHONE"),
            Self::InternationalPhone => write!(f, "PHONE"),
            Self::Email => write!(f, "EMAIL"),
            Self::ChineseIdCard => write!(f, "IDCARD"),
            Self::BankCard => write!(f, "BANKCARD"),
            Self::ChineseName => write!(f, "PERSON"),
            Self::IpAddress => write!(f, "IP"),
            Self::Url => write!(f, "URL"),
            Self::ApiKey => write!(f, "APIKEY"),
            Self::Password => write!(f, "PASSWORD"),
            Self::LicensePlate => write!(f, "PLATE"),
            Self::Passport => write!(f, "PASSPORT"),
            Self::SocialCredit => write!(f, "CREDITCODE"),
            Self::GeoCoordinate => write!(f, "GEO"),
            Self::PhysicalAddress => write!(f, "ADDRESS"),
            Self::MacAddress => write!(f, "MAC"),
            Self::Imei => write!(f, "IMEI"),
            Self::CryptoAddress => write!(f, "CRYPTO"),
            Self::CloudCredential => write!(f, "CLOUD_CRED"),
            Self::PrivateKey => write!(f, "PRIVATE_KEY"),
            Self::DbConnectionString => write!(f, "DB_CONN"),
            Self::JwtToken => write!(f, "JWT"),
            Self::Vin => write!(f, "VIN"),
            Self::Ipv6Address => write!(f, "IPV6"),
            Self::Iban => write!(f, "IBAN"),
            Self::Custom(name) => write!(f, "{}", name.to_uppercase()),
        }
    }
}
/// 检测到的 PII 实体
#[derive(Debug, Clone)]
pub struct PiiEntity {
    pub entity_type: PiiType,
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub confidence: f32,
}

struct CompiledPattern {
    regex: Regex,
    pii_type: PiiType,
    confidence: f32,
    validator: Option<fn(&str) -> bool>,
    needs_boundary: bool,
}

/// 多层 PII 检测器
pub struct PiiDetector {
    patterns: Vec<CompiledPattern>,
    chinese_surnames: HashSet<String>,
    dictionary: Option<Arc<SensitiveDictionary>>,
    context_detector: Option<ContextLeakDetector>,
}

impl Default for PiiDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl PiiDetector {
    pub fn new() -> Self {
        let patterns = Self::build_patterns();
        let chinese_surnames = Self::build_surname_dict();
        Self {
            patterns,
            chinese_surnames,
            dictionary: None,
            context_detector: None,
        }
    }

    /// 构建器：注入领域词典
    pub fn with_dictionary(mut self, dict: Arc<SensitiveDictionary>) -> Self {
        self.dictionary = Some(dict);
        self
    }

    /// 构建器：注入上下文泄露检测器
    pub fn with_context_detector(mut self, detector: ContextLeakDetector) -> Self {
        self.context_detector = Some(detector);
        self
    }

    /// 检测文本中的所有 PII 实体
    pub fn detect(&self, text: &str) -> Vec<PiiEntity> {
        let mut entities = Vec::new();

        // 1. 正则 + 校验算法
        for pat in &self.patterns {
            for m in pat.regex.find_iter(text) {
                let matched = m.as_str();
                if let Some(validator) = pat.validator {
                    if !validator(matched) {
                        continue;
                    }
                }
                // 边界检查：前后不能是同类字符（数字/字母）
                if pat.needs_boundary && !Self::check_boundary(text, m.start(), m.end()) {
                    continue;
                }
                entities.push(PiiEntity {
                    entity_type: pat.pii_type.clone(),
                    start: m.start(),
                    end: m.end(),
                    text: matched.to_string(),
                    confidence: pat.confidence,
                });
            }
        }

        // 2. 中文姓名字典匹配
        entities.extend(self.detect_chinese_names(text));

        // 3. 上下文密码检测
        entities.extend(Self::detect_passwords(text));

        // 4. 领域词典匹配
        if let Some(ref dict) = self.dictionary {
            entities.extend(dict.detect(text));
        }

        // 5. 上下文泄露检测
        if let Some(ref ctx) = self.context_detector {
            entities.extend(ctx.detect(text));
        }

        // 去重：按 (start, end) 去重，保留高置信度
        Self::deduplicate(&mut entities);
        entities
    }
    fn build_patterns() -> Vec<CompiledPattern> {
        vec![
            // 邮箱
            CompiledPattern {
                regex: Regex::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}").unwrap(),
                pii_type: PiiType::Email,
                confidence: 0.95,
                validator: None,
                needs_boundary: false,
            },
            // 中国手机号
            CompiledPattern {
                regex: Regex::new(r"1[3-9]\d[\s\-]?\d{4}[\s\-]?\d{4}").unwrap(),
                pii_type: PiiType::ChinesePhone,
                confidence: 0.95,
                validator: None,
                needs_boundary: true,
            },
            // 国际电话
            CompiledPattern {
                regex: Regex::new(r"\+\d{1,3}[\s\-]?\d[\d\s\-]{6,14}\d").unwrap(),
                pii_type: PiiType::InternationalPhone,
                confidence: 0.90,
                validator: None,
                needs_boundary: false,
            },
            // 中国身份证 (18位)
            CompiledPattern {
                regex: Regex::new(r"\d{6}(?:19|20)\d{2}(?:0[1-9]|1[0-2])(?:0[1-9]|[12]\d|3[01])\d{3}[\dXx]").unwrap(),
                pii_type: PiiType::ChineseIdCard,
                confidence: 0.95,
                validator: Some(Self::validate_id_card),
                needs_boundary: true,
            },
            // 银行卡 (16-19位)
            CompiledPattern {
                regex: Regex::new(r"\d{4}[\s\-]?\d{4}[\s\-]?\d{4}[\s\-]?\d{4}[\s\-]?\d{0,3}").unwrap(),
                pii_type: PiiType::BankCard,
                confidence: 0.85,
                validator: Some(Self::validate_luhn),
                needs_boundary: true,
            },
            // IPv4
            CompiledPattern {
                regex: Regex::new(r"(?:(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d\d?)").unwrap(),
                pii_type: PiiType::IpAddress,
                confidence: 0.90,
                validator: None,
                needs_boundary: true,
            },
            // URL
            CompiledPattern {
                regex: Regex::new(r#"https?://[^\s<>")]+"#).unwrap(),
                pii_type: PiiType::Url,
                confidence: 0.95,
                validator: None,
                needs_boundary: false,
            },
            // API Key 模式
            CompiledPattern {
                regex: Regex::new(r"(?:sk-[a-zA-Z0-9]{20,}|ghp_[a-zA-Z0-9]{36,}|Bearer\s+[a-zA-Z0-9\-._~+/]+=*)").unwrap(),
                pii_type: PiiType::ApiKey,
                confidence: 0.95,
                validator: None,
                needs_boundary: false,
            },
            // 车牌号
            CompiledPattern {
                regex: Regex::new(r"[京津沪渝冀豫云辽黑湘皖鲁新苏浙赣鄂桂甘晋蒙陕吉闽贵粤川青藏琼宁][A-HJ-NP-Z][A-HJ-NP-Z0-9]{4,5}[A-HJ-NP-Z0-9挂学警港澳]").unwrap(),
                pii_type: PiiType::LicensePlate,
                confidence: 0.95,
                validator: None,
                needs_boundary: false,
            },
            // 护照号
            CompiledPattern {
                regex: Regex::new(r"[EeGg]\d{8}").unwrap(),
                pii_type: PiiType::Passport,
                confidence: 0.80,
                validator: None,
                needs_boundary: true,
            },
            // 统一社会信用代码 (18位)
            CompiledPattern {
                regex: Regex::new(r"[0-9A-HJ-NP-RTUW-Y]{2}\d{6}[0-9A-HJ-NP-RTUW-Y]{10}").unwrap(),
                pii_type: PiiType::SocialCredit,
                confidence: 0.85,
                validator: None,
                needs_boundary: true,
            },
            // ── 新增结构化类型 ──────────────────────────────────
            // 地理坐标 (十进制度数)
            CompiledPattern {
                regex: Regex::new(r"[-+]?\d{1,3}\.\d{4,}\s*,\s*[-+]?\d{1,3}\.\d{4,}").unwrap(),
                pii_type: PiiType::GeoCoordinate,
                confidence: 0.90,
                validator: Some(Self::validate_geo_coordinate),
                needs_boundary: true,
            },
            // 地理坐标 (DMS 格式)
            CompiledPattern {
                regex: Regex::new(r#"\d{1,3}°\d{1,2}'\d{1,2}(?:\.\d+)?"[NSEW]\s*,?\s*\d{1,3}°\d{1,2}'\d{1,2}(?:\.\d+)?"[NSEW]"#).unwrap(),
                pii_type: PiiType::GeoCoordinate,
                confidence: 0.90,
                validator: None,
                needs_boundary: false,
            },
            // 中文物理地址
            CompiledPattern {
                regex: Regex::new(r"[\u{4e00}-\u{9fff}]{2,}(?:省|自治区)[\u{4e00}-\u{9fff}]{2,}(?:市|自治州)[\u{4e00}-\u{9fff}]*(?:区|县|市)[\u{4e00}-\u{9fff}]*(?:路|街|道|巷)[\u{4e00}-\u{9fff}0-9]*号").unwrap(),
                pii_type: PiiType::PhysicalAddress,
                confidence: 0.75,
                validator: None,
                needs_boundary: false,
            },
            // 英文物理地址
            CompiledPattern {
                regex: Regex::new(r"\d+\s+\w+\s+(?:St(?:reet)?|Ave(?:nue)?|Rd|Road|Blvd|Boulevard|Dr(?:ive)?|Ln|Lane|Ct|Court|Pl(?:ace)?|Way)\.?(?:\s*,\s*\w+)*").unwrap(),
                pii_type: PiiType::PhysicalAddress,
                confidence: 0.75,
                validator: None,
                needs_boundary: false,
            },
            // MAC 地址
            CompiledPattern {
                regex: Regex::new(r"[0-9A-Fa-f]{2}(?::[0-9A-Fa-f]{2}){5}").unwrap(),
                pii_type: PiiType::MacAddress,
                confidence: 0.95,
                validator: None,
                needs_boundary: true,
            },
            // MAC 地址 (连字符分隔)
            CompiledPattern {
                regex: Regex::new(r"[0-9A-Fa-f]{2}(?:-[0-9A-Fa-f]{2}){5}").unwrap(),
                pii_type: PiiType::MacAddress,
                confidence: 0.95,
                validator: None,
                needs_boundary: true,
            },
            // IMEI (15位数字)
            CompiledPattern {
                regex: Regex::new(r"\d{15}").unwrap(),
                pii_type: PiiType::Imei,
                confidence: 0.85,
                validator: Some(Self::validate_imei_luhn),
                needs_boundary: true,
            },
            // Bitcoin 地址 (Legacy)
            CompiledPattern {
                regex: Regex::new(r"[13][1-9A-HJ-NP-Za-km-z]{25,34}").unwrap(),
                pii_type: PiiType::CryptoAddress,
                confidence: 0.90,
                validator: None,
                needs_boundary: true,
            },
            // Bitcoin 地址 (Bech32)
            CompiledPattern {
                regex: Regex::new(r"bc1[a-z0-9]{25,39}").unwrap(),
                pii_type: PiiType::CryptoAddress,
                confidence: 0.90,
                validator: None,
                needs_boundary: true,
            },
            // Ethereum 地址
            CompiledPattern {
                regex: Regex::new(r"0x[0-9a-fA-F]{40}").unwrap(),
                pii_type: PiiType::CryptoAddress,
                confidence: 0.90,
                validator: None,
                needs_boundary: true,
            },
            // AWS Access Key
            CompiledPattern {
                regex: Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
                pii_type: PiiType::CloudCredential,
                confidence: 0.95,
                validator: None,
                needs_boundary: true,
            },
            // GCP 凭证
            CompiledPattern {
                regex: Regex::new(r"GOOG[\w]{10,30}").unwrap(),
                pii_type: PiiType::CloudCredential,
                confidence: 0.95,
                validator: None,
                needs_boundary: true,
            },
            // Azure 连接串
            CompiledPattern {
                regex: Regex::new(r"DefaultEndpointsProtocol=https?;[^\s]+").unwrap(),
                pii_type: PiiType::CloudCredential,
                confidence: 0.95,
                validator: None,
                needs_boundary: false,
            },
            // 私钥 (PEM)
            CompiledPattern {
                regex: Regex::new(r"-----BEGIN\s+(?:RSA\s+|EC\s+|DSA\s+|OPENSSH\s+)?PRIVATE\s+KEY-----").unwrap(),
                pii_type: PiiType::PrivateKey,
                confidence: 0.99,
                validator: None,
                needs_boundary: false,
            },
            // 数据库连接串
            CompiledPattern {
                regex: Regex::new(r"(?:postgres(?:ql)?|mysql|mongodb(?:\+srv)?|redis)://[^\s]+").unwrap(),
                pii_type: PiiType::DbConnectionString,
                confidence: 0.95,
                validator: None,
                needs_boundary: false,
            },
            // JWT Token
            CompiledPattern {
                regex: Regex::new(r"eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+").unwrap(),
                pii_type: PiiType::JwtToken,
                confidence: 0.90,
                validator: None,
                needs_boundary: true,
            },
            // VIN 车辆识别号
            CompiledPattern {
                regex: Regex::new(r"[A-HJ-NPR-Z0-9]{17}").unwrap(),
                pii_type: PiiType::Vin,
                confidence: 0.85,
                validator: Some(Self::validate_vin),
                needs_boundary: true,
            },
            // IPv6 地址
            CompiledPattern {
                regex: Regex::new(r"(?:[0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}|(?:[0-9a-fA-F]{1,4}:){1,7}:|(?:[0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}|::(?:[0-9a-fA-F]{1,4}:){0,5}[0-9a-fA-F]{1,4}|[0-9a-fA-F]{1,4}::(?:[0-9a-fA-F]{1,4}:){0,4}[0-9a-fA-F]{1,4}").unwrap(),
                pii_type: PiiType::Ipv6Address,
                confidence: 0.90,
                validator: None,
                needs_boundary: true,
            },
            // IBAN
            CompiledPattern {
                regex: Regex::new(r"[A-Z]{2}\d{2}\s?[\dA-Z]{4}\s?(?:[\dA-Z]{4}\s?){1,7}[\dA-Z]{1,4}").unwrap(),
                pii_type: PiiType::Iban,
                confidence: 0.90,
                validator: Some(Self::validate_iban),
                needs_boundary: true,
            },
        ]
    }
    /// 边界检查：匹配前后不能是字母数字字符
    fn check_boundary(text: &str, start: usize, end: usize) -> bool {
        if start > 0 {
            if let Some(prev) = text[..start].chars().last() {
                if prev.is_ascii_alphanumeric() {
                    return false;
                }
            }
        }
        if end < text.len() {
            if let Some(next) = text[end..].chars().next() {
                if next.is_ascii_alphanumeric() {
                    return false;
                }
            }
        }
        true
    }

    /// 身份证校验位验证
    fn validate_id_card(s: &str) -> bool {
        let digits: Vec<u32> = s.chars().take(17).filter_map(|c| c.to_digit(10)).collect();
        if digits.len() != 17 {
            return false;
        }
        let weights = [7, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2];
        let check_chars = ['1', '0', 'X', '9', '8', '7', '6', '5', '4', '3', '2'];
        let sum: u32 = digits.iter().zip(weights.iter()).map(|(d, w)| d * w).sum();
        let idx = (sum % 11) as usize;
        let Some(last) = s.chars().last() else {
            return false;
        };
        last.to_ascii_uppercase() == check_chars[idx]
    }

    /// Luhn 校验（银行卡）
    fn validate_luhn(s: &str) -> bool {
        let digits: Vec<u32> = s
            .chars()
            .filter(|c| c.is_ascii_digit())
            .filter_map(|c| c.to_digit(10))
            .collect();
        if digits.len() < 16 || digits.len() > 19 {
            return false;
        }
        let mut sum = 0u32;
        for (i, &d) in digits.iter().rev().enumerate() {
            if i % 2 == 1 {
                let doubled = d * 2;
                sum += if doubled > 9 { doubled - 9 } else { doubled };
            } else {
                sum += d;
            }
        }
        sum.is_multiple_of(10)
    }

    /// 地理坐标范围校验：lat ∈ [-90, 90], lng ∈ [-180, 180]
    fn validate_geo_coordinate(s: &str) -> bool {
        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() != 2 {
            return false;
        }
        let lat: f64 = match parts[0].trim().parse() {
            Ok(v) => v,
            Err(_) => return false,
        };
        let lng: f64 = match parts[1].trim().parse() {
            Ok(v) => v,
            Err(_) => return false,
        };
        (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lng)
    }

    /// IMEI Luhn 校验（15 位数字，最后一位为校验位）
    fn validate_imei_luhn(s: &str) -> bool {
        let digits: Vec<u32> = s.chars().filter_map(|c| c.to_digit(10)).collect();
        if digits.len() != 15 {
            return false;
        }
        let mut sum = 0u32;
        for (i, &d) in digits.iter().enumerate() {
            if i % 2 == 1 {
                let doubled = d * 2;
                sum += if doubled > 9 { doubled - 9 } else { doubled };
            } else {
                sum += d;
            }
        }
        sum.is_multiple_of(10)
    }

    /// VIN 第 9 位校验位验证
    fn validate_vin(s: &str) -> bool {
        if s.len() != 17 {
            return false;
        }
        let transliterate = |c: char| -> Option<u32> {
            match c {
                'A' | 'J' => Some(1),
                'B' | 'K' | 'S' => Some(2),
                'C' | 'L' | 'T' => Some(3),
                'D' | 'M' | 'U' => Some(4),
                'E' | 'N' | 'V' => Some(5),
                'F' | 'W' => Some(6),
                'G' | 'P' | 'X' => Some(7),
                'H' | 'Y' => Some(8),
                'R' | 'Z' => Some(9),
                '0'..='9' => c.to_digit(10),
                _ => None,
            }
        };
        let weights = [8, 7, 6, 5, 4, 3, 2, 10, 0, 9, 8, 7, 6, 5, 4, 3, 2];
        let chars: Vec<char> = s.chars().collect();
        let mut sum = 0u32;
        for (i, &c) in chars.iter().enumerate() {
            if i == 8 {
                continue;
            } // skip check digit position
            match transliterate(c) {
                Some(v) => sum += v * weights[i] as u32,
                None => return false,
            }
        }
        let remainder = sum % 11;
        let expected = if remainder == 10 {
            'X'
        } else {
            char::from_digit(remainder, 10).unwrap()
        };
        chars[8] == expected
    }

    /// IBAN mod-97 校验
    fn validate_iban(s: &str) -> bool {
        let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        if cleaned.len() < 5 {
            return false;
        }
        // 前两位必须是字母，第 3-4 位必须是数字
        let chars: Vec<char> = cleaned.chars().collect();
        if !chars[0].is_ascii_uppercase() || !chars[1].is_ascii_uppercase() {
            return false;
        }
        if !chars[2].is_ascii_digit() || !chars[3].is_ascii_digit() {
            return false;
        }
        // 将前 4 位移到末尾
        let rearranged = format!("{}{}", &cleaned[4..], &cleaned[..4]);
        // 字母转数字 (A=10, B=11, ..., Z=35)
        let mut numeric = String::new();
        for c in rearranged.chars() {
            if c.is_ascii_uppercase() {
                numeric.push_str(&((c as u32 - 'A' as u32 + 10).to_string()));
            } else {
                numeric.push(c);
            }
        }
        // mod 97 计算（分段避免溢出）
        let mut remainder = 0u64;
        for chunk in numeric.as_bytes().chunks(9) {
            let s: String = chunk.iter().map(|&b| b as char).collect();
            let combined = format!("{}{}", remainder, s);
            remainder = combined.parse::<u64>().unwrap_or(0) % 97;
        }
        remainder == 1
    }

    fn build_surname_dict() -> HashSet<String> {
        let surnames = [
            "赵", "钱", "孙", "李", "周", "吴", "郑", "王", "冯", "陈", "褚", "卫", "蒋", "沈",
            "韩", "杨", "朱", "秦", "尤", "许", "何", "吕", "施", "张", "孔", "曹", "严", "华",
            "金", "魏", "陶", "姜", "戚", "谢", "邹", "喻", "柏", "水", "窦", "章", "云", "苏",
            "潘", "葛", "奚", "范", "彭", "郎", "鲁", "韦", "昌", "马", "苗", "凤", "花", "方",
            "俞", "任", "袁", "柳", "酆", "鲍", "史", "唐", "费", "廉", "岑", "薛", "雷", "贺",
            "倪", "汤", "滕", "殷", "罗", "毕", "郝", "邬", "安", "常", "乐", "于", "时", "傅",
            "皮", "卞", "齐", "康", "伍", "余", "元", "卜", "顾", "孟", "平", "黄", "和", "穆",
            "萧", "尹", "姚", "邵", "湛", "汪", "祁", "毛", "禹", "狄", "米", "贝", "明", "臧",
            "计", "伏", "成", "戴", "谈", "宋", "茅", "庞", "熊", "纪", "舒", "屈", "项", "祝",
            "董", "梁", "杜", "阮", "蓝", "闵", "席", "季", "麻", "强", "贾", "路", "娄", "危",
            "江", "童", "颜", "郭", "梅", "盛", "林", "刁", "钟", "徐", "邱", "骆", "高", "夏",
            "蔡", "田", "樊", "胡", "凌", "霍", "虞", "万", "支", "柯", "昝", "管", "卢", "莫",
            "经", "房", "裘", "缪", "干", "解", "应", "宗", "丁", "宣", "贲", "邓", "郁", "单",
            "杭", "洪", "包", "诸", "左", "石", "崔", "吉", "钮", "龚", "程", "嵇", "邢", "滑",
            "裴", "陆", "荣", "翁", "荀", "羊", "於", "惠", "甄", "曲", "家", "封", "芮", "羿",
            "储", "靳", "汲", "邴", "糜", "松", "井", "段", "富", "巫", "乌", "焦", "巴", "弓",
            "牧", "隗", "山", "谷", "车", "侯", "宓", "蓬", "全", "郗", "班", "仰", "秋", "仲",
            "伊", "宫", "宁", "仇", "栾", "暴", "甘", "钭", "厉", "戎", "祖", "武", "符", "刘",
            "景", "詹", "束", "龙", "叶", "幸", "司", "韶", "郜", "黎", "蓟", "溥", "印", "宿",
            "白", "怀", "蒲", "邰", "从", "鄂", "索", "咸", "籍", "赖", "卓", "蔺", "屠", "蒙",
            "池", "乔", "阴", "胥", "能", "苍", "双", "闻", "莘", "党", "翟", "谭", "贡", "劳",
            "逄", "姬", "申", "扶", "堵", "冉", "宰", "郦", "雍", "却", "璩", "桑", "桂", "濮",
            "牛", "寿", "通", "边", "扈", "燕", "冀", "浦", "尚", "农", "温", "别", "庄", "晏",
            "柴", "瞿", "阎", "充", "慕", "连", "茹", "习", "宦", "艾", "鱼", "容", "向", "古",
            "易", "慎", "戈", "廖", "庾", "终", "暨", "居", "衡", "步", "都", "耿", "满", "弘",
            "匡", "国", "文", "寇", "广", "禄", "阙", "东", "欧", "殳", "沃", "利", "蔚", "越",
            "夔", "隆", "师", "巩", "厍", "聂", "晁", "勾", "敖", "融", "冷", "訾", "辛", "阚",
            "那", "简", "饶", "空", "曾", "毋", "沙", "乜", "养", "鞠", "须", "丰", "巢", "关",
            "蒯", "相", "查", "后", "荆", "红", "游", "竺", "权", "逯", "盖", "益", "桓", "公",
            "上官", "欧阳", "司马", "诸葛", "司徒", "司空", "令狐", "钟离", "宇文", "长孙", "慕容",
            "鲜于", "闾丘", "百里", "东方", "南宫", "万俟", "尉迟", "公孙", "端木", "皇甫", "轩辕",
            "太史", "夏侯", "独孤", "澹台", "拓跋", "呼延",
        ];
        surnames.iter().map(|s| s.to_string()).collect()
    }
    /// 中文姓名检测：百家姓 + 2-4字
    fn detect_chinese_names(&self, text: &str) -> Vec<PiiEntity> {
        let mut results = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        let mut i = 0;
        while i < len {
            // 尝试复姓 (2字) + 名 (1-2字)
            for surname_len in [2usize, 1] {
                if i + surname_len > len {
                    continue;
                }
                let surname: String = chars[i..i + surname_len].iter().collect();
                if !self.chinese_surnames.contains(&surname) {
                    continue;
                }
                for name_len in [2usize, 1] {
                    let total = surname_len + name_len;
                    if i + total > len {
                        continue;
                    }
                    let full: String = chars[i..i + total].iter().collect();
                    // 所有字符必须是中文
                    if !full.chars().all(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)) {
                        continue;
                    }
                    // 边界检查：前后不能是中文字符（避免从句子中间截取）
                    let prev_is_cjk = i > 0 && ('\u{4e00}'..='\u{9fff}').contains(&chars[i - 1]);
                    let next_is_cjk =
                        i + total < len && ('\u{4e00}'..='\u{9fff}').contains(&chars[i + total]);
                    if prev_is_cjk || next_is_cjk {
                        continue;
                    }
                    let byte_start = chars[..i].iter().map(|c| c.len_utf8()).sum();
                    let byte_end: usize = byte_start + full.len();
                    results.push(PiiEntity {
                        entity_type: PiiType::ChineseName,
                        start: byte_start,
                        end: byte_end,
                        text: full,
                        confidence: 0.70,
                    });
                }
            }
            i += 1;
        }
        results
    }

    /// 上下文密码检测
    fn detect_passwords(text: &str) -> Vec<PiiEntity> {
        let re =
            Regex::new(r#"(?i)(?:密码|password|passwd|pwd|secret|token)[\s:：=]+(\S+)"#).unwrap();
        let mut results = Vec::new();
        for caps in re.captures_iter(text) {
            if let Some(m) = caps.get(1) {
                results.push(PiiEntity {
                    entity_type: PiiType::Password,
                    start: m.start(),
                    end: m.end(),
                    text: m.as_str().to_string(),
                    confidence: 0.85,
                });
            }
        }
        results
    }

    fn deduplicate(entities: &mut Vec<PiiEntity>) {
        entities.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
        let mut keep = vec![true; entities.len()];
        for i in 0..entities.len() {
            if !keep[i] {
                continue;
            }
            for j in (i + 1)..entities.len() {
                if !keep[j] {
                    continue;
                }
                if entities[j].start < entities[i].end {
                    // 重叠：保留置信度高的
                    if entities[j].confidence > entities[i].confidence {
                        keep[i] = false;
                        break;
                    } else {
                        keep[j] = false;
                    }
                }
            }
        }
        let mut idx = 0;
        keep.iter().for_each(|&k| {
            if !k {
                entities.remove(idx);
            } else {
                idx += 1;
            }
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn detector() -> PiiDetector {
        PiiDetector::new()
    }

    #[test]
    fn detect_email() {
        let entities = detector().detect("联系 test@example.com 好的");
        assert!(entities.iter().any(|e| e.entity_type == PiiType::Email));
    }

    #[test]
    fn detect_chinese_phone() {
        let entities = detector().detect("手机号13812345678结束");
        assert!(entities
            .iter()
            .any(|e| e.entity_type == PiiType::ChinesePhone));
    }

    #[test]
    fn detect_id_card_with_checksum() {
        let text = "身份证 110101199001010031 的号码";
        let entities = detector().detect(text);
        assert!(entities
            .iter()
            .any(|e| e.entity_type == PiiType::ChineseIdCard));
    }

    #[test]
    fn detect_ip_address() {
        let entities = detector().detect("服务器 192.168.1.100 的端口");
        assert!(entities.iter().any(|e| e.entity_type == PiiType::IpAddress));
    }

    #[test]
    fn detect_url() {
        let entities = detector().detect("访问 https://example.com/path 获取");
        assert!(entities.iter().any(|e| e.entity_type == PiiType::Url));
    }

    #[test]
    fn detect_api_key() {
        let entities = detector().detect("key: sk-abcdefghijklmnopqrstuvwxyz");
        assert!(entities.iter().any(|e| e.entity_type == PiiType::ApiKey));
    }

    #[test]
    fn detect_license_plate() {
        let entities = detector().detect("车牌京A12345号");
        assert!(entities
            .iter()
            .any(|e| e.entity_type == PiiType::LicensePlate));
    }

    #[test]
    fn detect_password_context() {
        let entities = detector().detect("密码: mySecret123");
        assert!(entities
            .iter()
            .any(|e| e.entity_type == PiiType::Password && e.text == "mySecret123"));
    }

    #[test]
    fn no_pii_no_false_positive() {
        let entities = detector().detect("今天天气不错，适合写代码。");
        // 不应检测到高置信度的 PII（姓名可能低置信度）
        let high_conf: Vec<_> = entities.iter().filter(|e| e.confidence > 0.8).collect();
        assert!(high_conf.is_empty());
    }

    #[test]
    fn empty_text() {
        assert!(detector().detect("").is_empty());
    }

    #[test]
    fn mixed_pii() {
        let text = "用户 test@mail.com 手机 13912345678 IP 10.0.0.1";
        let entities = detector().detect(text);
        let types: HashSet<_> = entities.iter().map(|e| e.entity_type.to_string()).collect();
        assert!(types.contains("EMAIL"));
        assert!(types.contains("PHONE"));
        assert!(types.contains("IP"));
    }
}
