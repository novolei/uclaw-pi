import { useAtom } from 'jotai'
import * as DialogPrimitive from '@radix-ui/react-dialog'
import { motion, AnimatePresence } from 'motion/react'
import { settingsOpenAtom } from '@/atoms/settings-tab'
import SettingsPanel from './SettingsPanel'

const DIALOG_EASE: [number, number, number, number] = [0.32, 0.72, 0, 1]

/**
 * Settings dialog — motion-orchestrated so the exit animation fully
 * runs before the Radix Content unmounts. Pure opacity + a 0.8% scale
 * settle (no slide, no large zoom) — same motion language as the
 * shared dialog/alert-dialog primitives.
 */
export function SettingsDialog() {
  const [open, setOpen] = useAtom(settingsOpenAtom)

  return (
    <DialogPrimitive.Root open={open} onOpenChange={setOpen}>
      <AnimatePresence>
        {open && (
          <DialogPrimitive.Portal forceMount>
            <DialogPrimitive.Overlay
              forceMount
              className="fixed inset-0 z-[100]"
            >
              <motion.div
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                transition={{ duration: 0.22, ease: DIALOG_EASE }}
                className="absolute inset-0 bg-black/30 backdrop-blur-[2px]"
              />
            </DialogPrimitive.Overlay>
            <DialogPrimitive.Content
              forceMount
              className="fixed left-[50%] top-[50%] z-[100] translate-x-[-50%] translate-y-[-50%]"
            >
              <motion.div
                initial={{ opacity: 0, scale: 0.992 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.992 }}
                transition={{ duration: 0.22, ease: DIALOG_EASE }}
                style={{ width: 'min(85vw, 1200px)', height: 'min(85vh, 800px)' }}
                className="bg-background shadow-2xl rounded-2xl overflow-hidden"
              >
                <DialogPrimitive.Title className="sr-only">设置</DialogPrimitive.Title>
                <SettingsPanel />
              </motion.div>
            </DialogPrimitive.Content>
          </DialogPrimitive.Portal>
        )}
      </AnimatePresence>
    </DialogPrimitive.Root>
  )
}
