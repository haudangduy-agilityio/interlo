// Reads hex code points from stdin and reports which font Core Text picks for them when the
// primary font is the system UI font, and whether any code point has no glyph (a missing-glyph box).
import CoreText
import Foundation
let cps = (readLine(strippingNewline: true) ?? "").split(separator: " ").compactMap { UInt32($0, radix: 16) }
let system = CTFontCreateUIFontForLanguage(.system, 14, nil)!
print("system UI font:", CTFontCopyPostScriptName(system) as String, "family:", CTFontCopyFamilyName(system) as String)
var covered = 0
var fallbacks: [String: Int] = [:]
var noGlyph: [String] = []
for cp in cps {
  let s = String(UnicodeScalar(cp)!)
  var chars = Array(s.utf16)
  var glyphs = [CGGlyph](repeating: 0, count: chars.count)
  if CTFontGetGlyphsForCharacters(system, &chars, &glyphs, chars.count) {
    covered += 1
  } else {
    let f = CTFontCreateForString(system, s as CFString, CFRange(location: 0, length: chars.count))
    var g2 = [CGGlyph](repeating: 0, count: chars.count)
    if CTFontGetGlyphsForCharacters(f, &chars, &g2, chars.count) {
      fallbacks[CTFontCopyPostScriptName(f) as String, default: 0] += 1
    } else { noGlyph.append(String(format: "U+%04X", cp)) }
  }
}
print("code points:", cps.count, "held by the system UI font:", covered, "from another font:", fallbacks, "no glyph anywhere:", noGlyph)
