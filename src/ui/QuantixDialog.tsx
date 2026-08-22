import { Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";
import type { ReactNode } from "react";

interface QuantixDialogProps {
  isOpen: boolean;
  title: string;
  children: ReactNode;
  onOpenChange: (open: boolean) => void;
}

export function QuantixDialog({
  isOpen,
  title,
  children,
  onOpenChange,
}: QuantixDialogProps) {
  return (
    <ModalOverlay
      className="qx-ui-dialog__overlay"
      isOpen={isOpen}
      onOpenChange={onOpenChange}
    >
      <Modal className="qx-ui-dialog__modal">
        <Dialog className="qx-ui-dialog" aria-label={title}>
          <Heading slot="title">{title}</Heading>
          {children}
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
