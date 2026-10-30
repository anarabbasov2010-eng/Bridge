import {describe,expect,it} from "vitest";
import {clampSupportMessage,SUPPORT_MESSAGE_LIMIT,validateScreenshot} from "./validation";
describe("support message limits",()=>{
 it("keeps messages within 800 Unicode characters",()=>{expect(Array.from(clampSupportMessage("я".repeat(810))).length).toBe(SUPPORT_MESSAGE_LIMIT);});
 it("does not split emoji",()=>{const result=clampSupportMessage("🙂".repeat(801));expect(Array.from(result)).toHaveLength(800);expect(result).toBe("🙂".repeat(800));});
 it("rejects non-image attachments",()=>{const file=new File(["text"],"notes.txt",{type:"text/plain"});expect(validateScreenshot(file)).toBe("Choose an image file.");});
});
