import { EditorFontWeight } from './types'

export function cssFontWeight(weight: EditorFontWeight): string {
  return weight === EditorFontWeight.SEMIBOLD ? '600' : weight.toLowerCase()
}

/** Maps to CSS `-webkit-text-stroke` */
export interface TextStroke {
  width: number
  color: string
}

/** Maps to CSS `text-shadow: offsetX offsetY blur color` */
export interface TextShadow {
  offsetX: number
  offsetY: number
  blur: number
  color: string
}
