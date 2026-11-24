import {describe,it,expect} from "vitest";
import {validChoices,maySelect,type DesktopChoices} from "./desktopChoiceModel";
const value:DesktopChoices={token:"t",expires_in_ms:30000,choices:[{number:1,label:"Open",role:"button",container:"Toolbar"},{number:2,label:"Open",role:"button",container:"Dialog"}]};
describe("desktop control choices",()=>{
 it("rejects malformed objects without throwing",()=>{for(const v of [null,undefined,1,"x",{}, {...value, choices:[null,null]}, {...value,choices:[1,2]}, {...value,expires_in_ms:NaN}, {...value,expires_in_ms:Infinity}, {...value,expires_in_ms:30001}, {...value,token:"t".repeat(129)}])expect(validChoices(v)).toBe(false)});
 it("rejects long labels and out-of-order choices",()=>{expect(validChoices({...value,choices:[{...value.choices[0],label:"x".repeat(81)},value.choices[1]]})).toBe(false);expect(validChoices({...value,choices:[value.choices[1],value.choices[0]]})).toBe(false)});

 it("accepts only bounded numbered native choices",()=>{expect(validChoices(value)).toBe(true);for(const v of [{...value,expires_in_ms:0},{...value,choices:[value.choices[0]]},{...value,token:""},{...value,choices:[...value.choices,{...value.choices[1],number:2}]}])expect(validChoices(v)).toBe(false)});
 it("blocks expiry, double submission and invalid numbers",()=>{expect(maySelect(value,1,100,99,false)).toBe(true);expect(maySelect(value,1,100,100,false)).toBe(false);expect(maySelect(value,1,100,99,true)).toBe(false);expect(maySelect(value,3,100,99,false)).toBe(false);expect(maySelect(null,1,100,99,false)).toBe(false)});
});
