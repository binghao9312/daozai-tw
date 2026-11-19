//! 簡體轉臺灣繁體（只轉字形，不改用語）。
//!
//! 做法與 OpenCC 的 `s2tw` 相同，分兩個階段：先以詞組、再以單字做最長匹配把簡體轉成繁體，
//! 再把臺灣用字（例如 着→著）套在整段輸出上。詞庫出自 OpenCC（Apache-2.0），
//! 由 `scripts/gen-s2t.py` 產生到 `assets/s2t/`，內容與 OpenCC 完全一致。
//!
//! 完整詞組表（約 4.9 萬條）建起來要 3 MB 左右，而每首歌只需要轉一次，所以用 [`Converter::new`]
//! 要用時才建、用完即釋放，不常駐記憶體。
//!
//! 歌詞庫（LRCLIB）常見簡體歌詞，臺灣版統一顯示繁體；是否轉換由 [`Converter::mostly_simplified`] 判斷，
//! 已經是繁體的文字不會被碰，免得把「皇后」「公里」這類繁體裡本來就正確的字轉壞。
use std::collections::{HashMap,HashSet};

const CHARACTERS:&str=include_str!("../assets/s2t/characters.tsv");
const PHRASES:&str=include_str!("../assets/s2t/phrases.tsv");
const VARIANTS:&str=include_str!("../assets/s2t/variants.tsv");

pub struct Converter {
    characters:HashMap<char,&'static str>,
    phrases:HashMap<&'static str,&'static str>,
    /// 詞組的字數，由長到短；最長匹配時依序嘗試。
    phrase_lengths:Vec<usize>,
    variants:HashMap<char,&'static str>,
    /// 簡體特有的字：簡轉繁時會被改掉，而且不是任何繁體字的輸出。
    simplified:HashSet<char>,
    /// 繁體特有的字：是簡轉繁的輸出，本身不會再被改。
    traditional:HashSet<char>,
}

/// 詞庫每行「來源<TAB>繁體」；檔案在 Windows 上可能被換成 CRLF，所以去掉行尾的 `\r`。
fn pairs(text:&'static str)->impl Iterator<Item=(&'static str,&'static str)> {
    text.lines().filter_map(|line|line.trim_end_matches('\r').split_once('\t')).filter(|(key,value)|!key.is_empty()&&!value.is_empty())
}

/// 只取單字的條目。
fn single_characters(text:&'static str)->impl Iterator<Item=(char,&'static str)> {
    pairs(text).filter_map(|(key,value)|{let mut chars=key.chars();let first=chars.next()?;chars.next().is_none().then_some((first,value))})
}

impl Converter {
    /// 解析內嵌的詞庫並建表。
    pub fn new()->Self {
        let characters:HashMap<char,&'static str>=single_characters(CHARACTERS).collect();
        let phrases:HashMap<&'static str,&'static str>=pairs(PHRASES).collect();
        let mut phrase_lengths:Vec<usize>=phrases.keys().map(|key|key.chars().count()).collect();
        phrase_lengths.sort_unstable_by(|a,b|b.cmp(a));phrase_lengths.dedup();
        let variants=single_characters(VARIANTS).collect();
        let traditional:HashSet<char>=characters.values().flat_map(|value|value.chars()).filter(|c|!characters.contains_key(c)).collect();
        let simplified:HashSet<char>=characters.keys().copied().filter(|c|!traditional.contains(c)).collect();
        Self{characters,phrases,phrase_lengths,variants,simplified,traditional}
    }

    /// 簡體轉臺灣繁體：詞組優先、再逐字，最後套臺灣用字。
    pub fn to_traditional(&self,text:&str)->String {
        let chars:Vec<(usize,char)>=text.char_indices().collect();
        let mut converted=String::with_capacity(text.len());
        let mut index=0;
        'next:while index<chars.len() {
            for &length in &self.phrase_lengths {
                if index+length>chars.len() {continue;}
                let start=chars[index].0;
                let end=chars.get(index+length).map_or(text.len(),|(offset,_)|*offset);
                if let Some(phrase)=self.phrases.get(&text[start..end]) {
                    converted.push_str(phrase);index+=length;continue 'next;
                }
            }
            let ch=chars[index].1;
            match self.characters.get(&ch) {Some(traditional)=>converted.push_str(traditional),None=>converted.push(ch)}
            index+=1;
        }
        // 第二階段：臺灣用字套在整段輸出上（含沒被簡轉繁改動的字）。
        let mut out=String::with_capacity(converted.len());
        for ch in converted.chars() {
            match self.variants.get(&ch) {Some(variant)=>out.push_str(variant),None=>out.push(ch)}
        }
        out
    }

    /// 這些文字是不是以簡體為主：簡體特有字至少 3 個，且不少於繁體特有字的 2 倍。
    /// 「后」「里」「干」這類簡繁共用的字不會算成簡體特有字。
    pub fn mostly_simplified<'a>(&self,texts:impl IntoIterator<Item=&'a str>)->bool {
        let (mut simplified,mut traditional)=(0usize,0usize);
        for ch in texts.into_iter().flat_map(str::chars) {
            if self.simplified.contains(&ch) {simplified+=1;} else if self.traditional.contains(&ch) {traditional+=1;}
        }
        simplified>=3 && simplified>=traditional*2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_common_lyric_characters_and_keeps_ambiguous_ones_right() {
        let converter=Converter::new();
        for (simplified,traditional) in [
            ("我对着流星","我對著流星"),
            ("祈祷时专心","祈禱時專心"),
            ("为爱飞行 脱离地心引力的热情","為愛飛行 脫離地心引力的熱情"),
            ("万里无云 天很清","萬里無雲 天很清"),
            // 一個簡體字對應多個繁體字：靠詞組分辨。
            ("头发 发现 后来 皇后","頭髮 發現 後來 皇后"),
            ("干净 里面 一只鸟","乾淨 裡面 一隻鳥"),
            ("我们在这里等你","我們在這裡等你"),
            // 詞組之間不能互相搶字：「想念」不能被「念的」之類吃掉，「那只是」不能變成「那隻是」。
            ("想念的心 思念的旺季","想念的心 思念的旺季"),
            ("那只是偶尔醉意 别人不会了解","那只是偶爾醉意 別人不會了解"),
            // 臺灣用字：沒被簡轉繁改動的字也要套。
            ("着急 睡着了","著急 睡著了"),
            ("Hello 123, 你好!","Hello 123, 你好!"),
        ] {
            assert_eq!(converter.to_traditional(simplified),traditional,"{simplified}");
        }
        assert_eq!(converter.to_traditional(""),"");
    }

    #[test]
    fn already_traditional_text_is_left_alone_by_the_detector() {
        let converter=Converter::new();
        let traditional=["我對著流星","祈禱時專心","為愛飛行 脫離地心引力的熱情","皇后 公里 王后 天干"];
        assert!(!converter.mostly_simplified(traditional),"繁體不能被判成簡體");
        let simplified=["我对着流星","祈祷时专心","为爱飞行 脱离地心引力的热情"];
        assert!(converter.mostly_simplified(simplified));
        // 太短或只有一兩個簡體字：不判成簡體，免得亂轉。
        assert!(!converter.mostly_simplified(["爱你"]));
        assert!(!converter.mostly_simplified(["一二三 abc"]));
        // 簡繁混雜時看誰多：繁體為主的不轉。
        assert!(!converter.mostly_simplified(["我對著流星祈禱 為愛飛行 脫離地心引力","说","对","们"]));
    }

    #[test]
    fn table_covers_longest_phrases_without_panicking_on_boundaries() {
        let converter=Converter::new();
        // 詞組貼在字串兩端、連續詞組、非 CJK 夾雜，都不能越界或漏字。
        assert_eq!(converter.to_traditional("万里"),"萬里");
        assert_eq!(converter.to_traditional("万里无云万里"),"萬里無雲萬里");
        assert_eq!(converter.to_traditional("a头发b"),"a頭髮b");
        assert_eq!(converter.to_traditional("😀头发😀"),"😀頭髮😀");
        assert!(converter.phrase_lengths.windows(2).all(|pair|pair[0]>pair[1]),"長度要由長到短、不重複");
        assert!(converter.characters.len()>3000&&converter.phrases.len()>40000&&!converter.variants.is_empty());
    }
}
