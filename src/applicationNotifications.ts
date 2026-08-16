import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

export async function enableAttentionNotifications(): Promise<boolean> {
  if (await isPermissionGranted()) return true;
  return (await requestPermission()) === "granted";
}

export async function notifyAttentionRequired(): Promise<void> {
  if (!(await isPermissionGranted())) return;
  sendNotification({
    title: "Quantix needs your attention",
    body: "Open Quantix to review the Tendering Manager's request.",
  });
}
