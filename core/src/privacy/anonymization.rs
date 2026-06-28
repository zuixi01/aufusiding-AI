//! 数据匿名化
//!
//! 检测并替换文本中的 PII（个人身份信息）。

/// PII 匿名化器
pub struct Anonymizer;

impl Anonymizer {
    /// 匿名化文本中的 PII
    pub fn anonymize(text: &str) -> String {
        let mut result = text.to_string();
        result = Self::replace_emails(&result);
        result = Self::replace_chinese_phones(&result);
        result = Self::replace_international_phones(&result);
        result = Self::replace_id_cards(&result);
        result
    }

    /// 替换邮箱地址
    fn replace_emails(text: &str) -> String {
        let mut result = String::new();
        let mut i = 0;
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();

        while i < len {
            // 寻找 @ 符号
            if chars[i] == '@' && i > 0 {
                // 回溯找到邮箱开始位置
                let mut start = i;
                while start > 0 {
                    let c = chars[start - 1];
                    if c.is_alphanumeric() || c == '.' || c == '_' || c == '-' || c == '+' {
                        start -= 1;
                    } else {
                        break;
                    }
                }
                // 前进找到域名结束位置
                let mut end = i + 1;
                let mut has_dot = false;
                while end < len {
                    let c = chars[end];
                    if c.is_alphanumeric() || c == '.' || c == '-' {
                        if c == '.' {
                            has_dot = true;
                        }
                        end += 1;
                    } else {
                        break;
                    }
                }

                if start < i && has_dot && end > i + 1 {
                    // 移除已添加的本地部分
                    let local_len: usize = chars[start..i].iter().map(|c| c.len_utf8()).sum();
                    let result_bytes = result.len();
                    result.truncate(result_bytes - local_len);
                    result.push_str("[EMAIL]");
                    i = end;
                    continue;
                }
            }
            result.push(chars[i]);
            i += 1;
        }
        result
    }

    /// 替换中国手机号 (1xx-xxxx-xxxx 或 1xxxxxxxxxx)
    fn replace_chinese_phones(text: &str) -> String {
        let mut result = String::new();
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            if chars[i] == '1' && i + 1 < len && chars[i + 1].is_ascii_digit() {
                // 收集连续数字和分隔符
                let start = i;
                let mut digits = Vec::new();
                let mut j = i;
                while j < len {
                    if chars[j].is_ascii_digit() {
                        digits.push(chars[j]);
                        j += 1;
                    } else if (chars[j] == '-' || chars[j] == ' ') && !digits.is_empty() {
                        j += 1;
                    } else {
                        break;
                    }
                }

                if digits.len() == 11 && digits[0] == '1' {
                    let second = digits[1];
                    if second == '3'
                        || second == '4'
                        || second == '5'
                        || second == '6'
                        || second == '7'
                        || second == '8'
                        || second == '9'
                    {
                        // 检查前一个字符不是数字
                        let prev_is_digit = start > 0 && chars[start - 1].is_ascii_digit();
                        // 检查后一个字符不是数字
                        let next_is_digit = j < len && chars[j].is_ascii_digit();

                        if !prev_is_digit && !next_is_digit {
                            result.push_str("[PHONE]");
                            i = j;
                            continue;
                        }
                    }
                }
            }
            result.push(chars[i]);
            i += 1;
        }
        result
    }

    /// 替换国际电话号码 (+xx xxx...)
    fn replace_international_phones(text: &str) -> String {
        let mut result = String::new();
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            if chars[i] == '+' && i + 1 < len && chars[i + 1].is_ascii_digit() {
                let start = i;
                let mut j = i + 1;
                let mut digit_count = 0;
                while j < len {
                    if chars[j].is_ascii_digit() {
                        digit_count += 1;
                        j += 1;
                    } else if (chars[j] == '-' || chars[j] == ' ') && digit_count > 0 {
                        j += 1;
                    } else {
                        break;
                    }
                }
                if digit_count >= 7 && digit_count <= 15 {
                    let _ = start;
                    result.push_str("[PHONE]");
                    i = j;
                    continue;
                }
            }
            result.push(chars[i]);
            i += 1;
        }
        result
    }

    /// 替换中国身份证号 (18位)
    fn replace_id_cards(text: &str) -> String {
        let mut result = String::new();
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            if chars[i].is_ascii_digit() && i + 17 < len {
                let prev_is_alnum = i > 0 && (chars[i - 1].is_alphanumeric());
                if !prev_is_alnum {
                    let candidate: String = chars[i..i + 18].iter().collect();
                    if Self::is_id_card(&candidate) {
                        let next_is_alnum = i + 18 < len && chars[i + 18].is_alphanumeric();
                        if !next_is_alnum {
                            result.push_str("[ID_CARD]");
                            i += 18;
                            continue;
                        }
                    }
                }
            }
            result.push(chars[i]);
            i += 1;
        }
        result
    }

    fn is_id_card(s: &str) -> bool {
        if s.len() != 18 {
            return false;
        }
        let chars: Vec<char> = s.chars().collect();
        // 前17位必须是数字
        for c in &chars[..17] {
            if !c.is_ascii_digit() {
                return false;
            }
        }
        // 第18位可以是数字或 X/x
        let last = chars[17];
        last.is_ascii_digit() || last == 'X' || last == 'x'
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anonymize_email() {
        let text = "联系我: test@example.com 谢谢";
        let result = Anonymizer::anonymize(text);
        assert_eq!(result, "联系我: [EMAIL] 谢谢");
    }

    #[test]
    fn anonymize_multiple_emails() {
        let text = "a@b.com and c@d.org";
        let result = Anonymizer::anonymize(text);
        assert!(result.contains("[EMAIL]"));
        assert!(!result.contains("a@b.com"));
        assert!(!result.contains("c@d.org"));
    }

    #[test]
    fn anonymize_chinese_phone() {
        let text = "我的手机号是13812345678";
        let result = Anonymizer::anonymize(text);
        assert_eq!(result, "我的手机号是[PHONE]");
    }

    #[test]
    fn anonymize_phone_with_dashes() {
        let text = "电话: 138-1234-5678";
        let result = Anonymizer::anonymize(text);
        assert!(result.contains("[PHONE]"));
        assert!(!result.contains("138"));
    }

    #[test]
    fn anonymize_international_phone() {
        let text = "call +86 13812345678";
        let result = Anonymizer::anonymize(text);
        assert!(result.contains("[PHONE]"));
    }

    #[test]
    fn anonymize_id_card() {
        let text = "身份证号: 110101199001011234";
        let result = Anonymizer::anonymize(text);
        assert!(result.contains("[ID_CARD]"));
        assert!(!result.contains("110101199001011234"));
    }

    #[test]
    fn anonymize_id_card_with_x() {
        let text = "ID: 11010119900101123X";
        let result = Anonymizer::anonymize(text);
        assert!(result.contains("[ID_CARD]"));
    }

    #[test]
    fn no_pii_unchanged() {
        let text = "这是一段普通文本，没有任何个人信息。";
        let result = Anonymizer::anonymize(text);
        assert_eq!(result, text);
    }

    #[test]
    fn mixed_pii() {
        let text = "用户 test@mail.com 手机 13912345678 身份证 110101199001011234";
        let result = Anonymizer::anonymize(text);
        assert!(result.contains("[EMAIL]"));
        assert!(result.contains("[PHONE]"));
        assert!(result.contains("[ID_CARD]"));
    }

    #[test]
    fn empty_text() {
        assert_eq!(Anonymizer::anonymize(""), "");
    }

    #[test]
    fn short_number_not_phone() {
        let text = "编号12345";
        let result = Anonymizer::anonymize(text);
        assert_eq!(result, text);
    }
}
