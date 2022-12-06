use analysis_yomikae::{LawInfo, YomikaeError, YomikaeInfo};
use anyhow::Result;
use clap::Parser;
use jplaw_text::LawContents;
use regex::Regex;
use search_article_with_word::{self, Chapter};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use tokio::{
  self,
  fs::File,
  io::{AsyncReadExt, AsyncWriteExt},
};
use tokio_stream::StreamExt;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
  /// 解析結果を出力するJSONファイルへのpath
  #[clap(short, long)]
  output: String,
  /// 解析に失敗したときの情報を出力しているjsonファイルへのpath
  #[clap(short, long)]
  error_input: String,
  /// 解析に成功した結果のjsonファイルへのpath
  #[clap(short, long)]
  analysis_input: String,
  /// `search_article_toyomikaeru.json`ファイルのように、読み替え規定文の疑いのあるデータのリストのファイルへのpath
  #[clap(short, long)]
  chapter_list: String,
}

#[tokio::main]
async fn main() -> Result<()> {
  let args = Args::parse();

  let mut analysis_f = File::open(args.analysis_input).await?;
  let mut analysis_buffer = Vec::new();
  analysis_f.read_to_end(&mut analysis_buffer).await?;
  let analysis_data_lst: Vec<YomikaeInfo> =
    serde_json::from_str(std::str::from_utf8(&analysis_buffer)?)?;
  let mut analysis_data_lst_tmp = analysis_data_lst
    .iter()
    .map(|d| (&d.num, &d.chapter))
    .collect::<Vec<_>>();
  analysis_data_lst_tmp.sort();
  analysis_data_lst_tmp.dedup();
  let size_of_analysis = analysis_data_lst_tmp.len();

  let mut error_f = File::open(args.error_input).await?;
  let mut error_buffer = Vec::new();
  error_f.read_to_end(&mut error_buffer).await?;
  let mut error_data_lst: Vec<YomikaeError> =
    serde_json::from_str(std::str::from_utf8(&error_buffer)?)?;
  error_data_lst.sort_by(ord_yomikae_error);
  error_data_lst.dedup_by(|a, b| is_eq_yomikae_error(a, b));
  let mut size_of_analysis_error_all = 0;
  let mut contents_of_table = 0;
  let mut unmatched_parenthese = 0;
  let mut unexpected_parallel_words = 0;
  let mut analysis_error_data_contents_of_table = Vec::new();
  let mut analysis_error_data_unmatched_parenthese = Vec::new();
  let mut analysis_error_data_unexpected_parallel_words = Vec::new();
  let mut size_of_not_analysis_yomikae_sentence = 0;
  let mut size_of_not_analysis_yomikae_sentence_in_kagi = 0;
  let mut size_of_not_analysis_yomikae_sentence_table = 0;
  let mut size_of_not_analysis_yomikae_sentence_not_contains = 0;
  let mut analysis_not_found_sentence_data_other = Vec::new();
  let mut error_data_stream = tokio_stream::iter(error_data_lst);
  while let Some(yomikae_error) = error_data_stream.next().await {
    match yomikae_error {
      YomikaeError::ContentsOfTable(info) => {
        size_of_analysis_error_all += 1;
        contents_of_table += 1;
        analysis_error_data_contents_of_table.push(info)
      }
      YomikaeError::UnmatchedParen(info) => {
        size_of_analysis_error_all += 1;
        unmatched_parenthese += 1;
        analysis_error_data_unmatched_parenthese.push(info)
      }
      YomikaeError::UnexpectedParallelWords(info) => {
        size_of_analysis_error_all += 1;
        unexpected_parallel_words += 1;
        analysis_error_data_unexpected_parallel_words.push(info)
      }
      YomikaeError::NotFoundYomikae(info) => {
        size_of_not_analysis_yomikae_sentence += 1;
        let law_contents = &info.contents.contents;
        let text = match law_contents {
          LawContents::Text(str) => str,
          _ => unreachable!(),
        };
        let in_kagi_kakko_re = Regex::new(r"[^「」]*「[^」]+と読み替える[^「」]*」.*").unwrap();
        if !text.contains("と読み替える") {
          size_of_not_analysis_yomikae_sentence_not_contains += 1;
        } else if in_kagi_kakko_re.is_match(text) {
          size_of_not_analysis_yomikae_sentence_in_kagi += 1;
        } else if text.contains("それぞれ同表の下欄に掲げる字句と読み替える")
          || text.contains("同表下欄の字句と読み替える")
          || text.contains("同表の下欄の字句と読み替える")
          || text.contains("下欄に掲げる字句と読み替える")
          || text.contains("同表の下欄に掲げる日又は月と読み替える")
        {
          size_of_not_analysis_yomikae_sentence_table += 1;
        } else {
          analysis_not_found_sentence_data_other.push(info)
        }
      }
    }
  }
  let size_of_analysis_error = SizeOfAnalysisError {
    all: size_of_analysis_error_all,
    contents_of_table,
    unmatched_parenthese,
    unexpected_parallel_words,
  };

  let size_of_not_analysis_yomikae_sentence = SizeOfNotFoundYomikae {
    all: size_of_not_analysis_yomikae_sentence,
    in_kagi_paren: size_of_not_analysis_yomikae_sentence_in_kagi,
    table_contents: size_of_not_analysis_yomikae_sentence_table,
    not_contains_toyomikaeru: size_of_not_analysis_yomikae_sentence_not_contains,
    other: size_of_not_analysis_yomikae_sentence
      - size_of_not_analysis_yomikae_sentence_in_kagi
      - size_of_not_analysis_yomikae_sentence_table
      - size_of_not_analysis_yomikae_sentence_not_contains,
  };

  let data_of_analysis_yomikae = DataOfAnalysisYomikae {
    size_of_all: size_of_analysis
      + size_of_analysis_error.all
      + size_of_not_analysis_yomikae_sentence.all,
    size_of_analysis,
    size_of_analysis_error,
    size_of_not_analysis_yomikae_sentence,
    analysis_error_data_contents_of_table,
    analysis_error_data_unmatched_parenthese,
    analysis_error_data_unexpected_parallel_words,
    analysis_not_found_sentence_data_other,
  };

  let mut file = File::create(args.output).await?;
  file
    .write_all(serde_json::to_string_pretty(&data_of_analysis_yomikae)?.as_bytes())
    .await?;
  Ok(())
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize, Copy)]
pub struct SizeOfAnalysisError {
  all: usize,
  contents_of_table: usize,
  unmatched_parenthese: usize,
  unexpected_parallel_words: usize,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize, Copy)]
pub struct SizeOfNotFoundYomikae {
  pub all: usize,
  /// 鍵括弧内にある場合
  pub in_kagi_paren: usize,
  /// 別表参照の場合
  pub table_contents: usize,
  /// そもそもとして「と読み替える」を含んでいない
  pub not_contains_toyomikaeru: usize,
  /// その他
  pub other: usize,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub struct LawSentenceInfo {
  num: String,
  chapter: Chapter,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub struct DataOfAnalysisYomikae {
  pub size_of_all: usize,
  pub size_of_analysis: usize,
  pub size_of_analysis_error: SizeOfAnalysisError,
  pub size_of_not_analysis_yomikae_sentence: SizeOfNotFoundYomikae,
  pub analysis_error_data_contents_of_table: Vec<LawInfo>,
  pub analysis_error_data_unmatched_parenthese: Vec<LawInfo>,
  pub analysis_error_data_unexpected_parallel_words: Vec<LawInfo>,
  pub analysis_not_found_sentence_data_other: Vec<LawInfo>,
}

fn ord_yomikae_error(a: &YomikaeError, b: &YomikaeError) -> Ordering {
  let a_law_info = get_law_info_from_yomikae_error(a);
  let b_law_info = get_law_info_from_yomikae_error(b);
  if a_law_info.num < b_law_info.num {
    Ordering::Less
  } else if a_law_info.num > b_law_info.num {
    Ordering::Greater
  } else if a_law_info.chapter < b_law_info.chapter {
    Ordering::Less
  } else if a_law_info.chapter > b_law_info.chapter {
    Ordering::Greater
  } else {
    Ordering::Equal
  }
}

fn get_law_info_from_yomikae_error(err: &YomikaeError) -> LawInfo {
  match err {
    YomikaeError::ContentsOfTable(info) => info.clone(),
    YomikaeError::UnmatchedParen(info) => info.clone(),
    YomikaeError::UnexpectedParallelWords(info) => info.clone(),
    YomikaeError::NotFoundYomikae(info) => info.clone(),
  }
}

fn is_eq_yomikae_error(a: &YomikaeError, b: &YomikaeError) -> bool {
  let a_law_info = get_law_info_from_yomikae_error(a);
  let b_law_info = get_law_info_from_yomikae_error(b);
  a_law_info.num == b_law_info.num && a_law_info.chapter == b_law_info.chapter
}
