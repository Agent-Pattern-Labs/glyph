import type { CompressionExample } from "./examples.js";

export interface CompressionStats {
  naturalLanguageChars: number;
  glyphChars: number;
  naturalLanguageWords: number;
  glyphWords: number;
  naturalLanguageApproxTokens: number;
  glyphApproxTokens: number;
  compressionRatio: number;
}

export function compareCompression(glyphSource: string, example: CompressionExample): CompressionStats {
  const naturalLanguage = example.naturalLanguage.trim();
  const glyph = glyphSource.trim();

  return {
    naturalLanguageChars: naturalLanguage.length,
    glyphChars: glyph.length,
    naturalLanguageWords: countWords(naturalLanguage),
    glyphWords: countWords(glyph),
    naturalLanguageApproxTokens: approximateTokens(naturalLanguage),
    glyphApproxTokens: approximateTokens(glyph),
    compressionRatio: naturalLanguage.length / Math.max(1, glyph.length)
  };
}

export function countWords(text: string): number {
  const words = text.trim().match(/\S+/g);
  return words ? words.length : 0;
}

export function approximateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}
