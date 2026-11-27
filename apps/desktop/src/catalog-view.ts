export type CatalogEntry={parts:string[];directory:boolean;size:number};
export type CatalogRow={entry:CatalogEntry;index:number};
export type CatalogSort="name"|"size";
export function sameParts(a:readonly string[],b:readonly string[]):boolean{return a.length===b.length&&a.every((p,i)=>p===b[i]);}
export function catalogRows(entries:readonly CatalogEntry[],folder:readonly string[],query:string,sort:CatalogSort="name"):CatalogRow[]{
 const prefix=(parts:string[])=>folder.every((p,i)=>parts[i]===p);
 const needle=query.trim().toLowerCase();
 return entries.map((entry,index)=>({entry,index})).filter(({entry:e})=>prefix(e.parts)&&e.parts.length>folder.length&&
    (needle?e.parts.slice(folder.length).join("/").toLowerCase().includes(needle):e.parts.length===folder.length+1))
   .sort((a,b)=>Number(b.entry.directory)-Number(a.entry.directory)||
      (sort==="size"?a.entry.size-b.entry.size:0)||a.entry.parts.join("/").localeCompare(b.entry.parts.join("/"))||a.index-b.index);
}
export function validFolder(entries:readonly CatalogEntry[],parts:readonly string[]):boolean{
 return parts.length===0||entries.some(e=>e.directory&&sameParts(e.parts,parts));
}
export function selectedRows(entries:readonly CatalogEntry[],indexes:readonly number[]):CatalogRow[]{
 return [...new Set(indexes)].filter(i=>Number.isInteger(i)&&i>=0&&i<entries.length&&!entries[i].directory).map(index=>({index,entry:entries[index]}));
}
export function toggleCatalogFile(entries:readonly CatalogEntry[],indexes:readonly number[],index:number,limit:1|4):number[]{
 const current=selectedRows(entries,indexes).map(r=>r.index);
 if(!Number.isInteger(index)||index<0||index>=entries.length||entries[index].directory)return current;
 if(current.includes(index))return current.filter(i=>i!==index);
 if(limit===1)return [index];
 return current.length<limit?[...current,index]:current;
}
