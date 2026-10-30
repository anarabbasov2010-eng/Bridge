export const SUPPORT_MESSAGE_LIMIT=800;
export const SUPPORT_SCREENSHOT_LIMIT=5;
export const SUPPORT_SCREENSHOT_MAX_BYTES=8*1024*1024;
export function clampSupportMessage(value:string):string{return Array.from(value).slice(0,SUPPORT_MESSAGE_LIMIT).join("");}
export function validateScreenshot(file:File):string|null{
 if(!file.type.startsWith("image/"))return "Choose an image file.";
 if(file.size>SUPPORT_SCREENSHOT_MAX_BYTES)return "Each image must be 8 MB or smaller.";
 return null;
}
export function formatBytes(bytes:number):string{
 if(bytes<1024)return `${bytes} B`;
 if(bytes<1024*1024)return `${(bytes/1024).toFixed(0)} KB`;
 return `${(bytes/(1024*1024)).toFixed(1)} MB`;
}
