import type { IEventClose } from './event-close'

export type GuardFunction = (e: IEventClose) => void | boolean | Promise<boolean>
export type GuardFunctionPromisify = () => Promise<void>

export interface ModalComponentInterface {
  onEscape?: () => void | Promise<void>
  [name: string]: any
}
