// Real projection used by FileCatalog.vue. Node 22 strips type declarations.
import assert from "node:assert/strict";
import {catalogRows,selectedRows,toggleCatalogFile,validFolder} from "../../apps/desktop/src/catalog-view.ts";
const entries=[
 {parts:["work"],directory:true,size:0},{parts:["work","z.txt"],directory:false,size:11},
 {parts:["a.txt"],directory:false,size:5},{parts:["work","nested"],directory:true,size:0},
 {parts:["work","nested","a.txt"],directory:false,size:1},{parts:["work2"],directory:true,size:0},
 {parts:["work2","x.txt"],directory:false,size:9},{parts:["b.txt"],directory:false,size:8},
 {parts:["c.txt"],directory:false,size:7}
];
assert.deepEqual(catalogRows(entries,[],"").map(r=>r.index),[0,5,2,7,8]);
assert.deepEqual(catalogRows(entries,["work"],"").map(r=>r.index),[3,1]);
assert.deepEqual(catalogRows(entries,["work"],"a.txt").map(r=>r.index),[4]);
assert.deepEqual(catalogRows(entries,[],"","size").map(r=>r.index),[0,5,2,8,7]);
assert.equal(validFolder(entries,["work","nested"]),true);
assert.equal(validFolder(entries,["work","z.txt"]),false);
assert.deepEqual(selectedRows(entries,[4,2,4,0,-1,100]).map(r=>r.index),[4,2]);
assert.deepEqual(toggleCatalogFile(entries,[1],4,1),[4]);
assert.deepEqual(toggleCatalogFile(entries,[1,2,4,6],8,4),[1,2,4,6]);
assert.deepEqual(toggleCatalogFile(entries,[1,2,4,6],4,4),[1,2,6]);
assert.deepEqual(toggleCatalogFile(entries,[1],0,4),[1]);
console.log("PASS catalog browsing, scoped search, sort and immutable protocol indexes; single and four-file limits");
