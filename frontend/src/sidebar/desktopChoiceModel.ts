export interface DesktopChoice {number:number;label:string;role:string;container:string}
export interface DesktopChoices {token:string;expires_in_ms:number;choices:DesktopChoice[]}
export function validChoices(value:unknown):value is DesktopChoices {
  if(!value || typeof value!=="object")return false;
  const v=value as DesktopChoices;
  return typeof v.token==="string" && v.token.length>0 && v.token.length<=128 &&
    Number.isFinite(v.expires_in_ms)&&v.expires_in_ms>0&&v.expires_in_ms<=30_000&&
    Array.isArray(v.choices)&&v.choices.length>=2&&v.choices.length<=5&&
    v.choices.every((c,i)=>!!c&&typeof c==="object"&&c.number===i+1&&[c.label,c.role,c.container].every(x=>typeof x==="string"&&x.length<=80));
}
export function maySelect(v:DesktopChoices|null,number:number,deadline:number,now:number,busy:boolean):boolean {
  return !!v&&!busy&&now<deadline&&v.choices.some(c=>c.number===number);
}
