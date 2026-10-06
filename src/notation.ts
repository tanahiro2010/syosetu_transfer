import { Fragment, createElement, type ReactNode } from "react";
import type { Platform, RubyMode } from "./types";

// 取得した本文は次の記法で保存されている（Rust 側の fetch.rs で変換済み）
//   ルビ:     |親文字《ルビ》
//   傍点:     《《文字》》（カクヨムの傍点）
//   地の文の《: |《
const DOTS = /《《([^《》\n]+?)》》/g;
const RUBY = /[|｜]([^|｜《》\n]+?)《([^《》\n]+?)》/g;
const ESCAPED_BRACKET = /[|｜]《/g;
// 親文字と同じ数の「・」を振ったルビ（なろう流の傍点）。1 文字ずつ振ったものが続く場合もまとめる
const DOT_RUBY_RUN = /(?:[|｜][^|｜《》\n]+?《[・﹅●]+》)+/g;
const NAROU_RUBY_MAX = 10;
// なろうでは「漢字（かな）」が自動でルビになるので、地の文のものは | を挟んで止める
const NAROU_AUTO_RUBY = /([一-鿿々〆ヶ])([（(])([ぁ-ゖァ-ヺー・]+[）)])/g;

/** 投稿先の記法に合わせて本文を変換する */
export function toPlatformNotation(text: string, target: Platform, mode: RubyMode): string {
  if (mode === "strip") {
    return text.replace(DOTS, "$1").replace(RUBY, "$1").replace(ESCAPED_BRACKET, "《");
  }

  if (target === "narou") {
    // なろうには傍点の記法がないため「|文章《・・》」のように文字数分の「・」をルビで振る。
    // ルビは 10 文字までなので、長い傍点は 10 文字ずつに分ける
    return text.replace(NAROU_AUTO_RUBY, "$1|$2$3").replace(DOTS, (_, phrase: string) => {
      const chars = [...phrase];
      const chunks: string[] = [];
      for (let start = 0; start < chars.length; start += NAROU_RUBY_MAX) {
        const chunk = chars.slice(start, start + NAROU_RUBY_MAX);
        chunks.push(`|${chunk.join("")}《${"・".repeat(chunk.length)}》`);
      }
      return chunks.join("");
    });
  }

  // カクヨム: なろう流の「・」ルビによる傍点を、カクヨムの傍点記法にまとめる
  return text.replace(DOT_RUBY_RUN, (run) => {
    const parts = [...run.matchAll(RUBY)];
    // 「・」の数が親文字の数と違うものは傍点ではなく普通のルビとみなしてそのまま残す
    const isDots = parts.every(([, base, ruby]) => [...base].length === [...ruby].length);
    return isDots ? `《《${parts.map((part) => part[1]).join("")}》》` : run;
  });
}

/** プレビュー用に、記法をルビ・傍点付きの要素として描画する */
export function renderNotation(text: string): ReactNode[] {
  const pattern = /《《([^《》\n]+?)》》|[|｜]([^|｜《》\n]+?)《([^《》\n]+?)》|[|｜](《)/g;
  const nodes: ReactNode[] = [];
  let last = 0;
  for (const match of text.matchAll(pattern)) {
    const index = match.index ?? 0;
    if (index > last) nodes.push(text.slice(last, index));
    const key = `${index}`;
    if (match[1] !== undefined) {
      nodes.push(createElement("em", { key, className: "emphasis-dots" }, match[1]));
    } else if (match[2] !== undefined) {
      nodes.push(createElement("ruby", { key }, match[2], createElement("rt", null, match[3])));
    } else {
      nodes.push(createElement(Fragment, { key }, match[4]));
    }
    last = index + match[0].length;
  }
  if (last < text.length) nodes.push(text.slice(last));
  return nodes;
}

/** 投稿先で表示が崩れそうなルビの数を数える（なろうは親文字・ルビとも 10 文字まで） */
export function countLongRuby(text: string, target: Platform): number {
  if (target !== "narou") return 0;
  return [...text.matchAll(RUBY)].filter(([, base, ruby]) => [...base].length > 10 || [...ruby].length > 10).length;
}
