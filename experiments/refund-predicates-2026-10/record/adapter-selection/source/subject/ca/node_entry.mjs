// Instrumented entry: explicit owner configuration and allocations; no default release.
import * as fs from 'node:fs';
import * as path from 'node:path';
import {createHash} from 'node:crypto';
import {pathToFileURL} from 'node:url';
import * as childProcess from 'node:child_process';

const digest = raw => createHash('sha256').update(raw).digest('hex');
export function readPinned(pin, cap=65536, io=fs) {
  if (!pin || Object.keys(pin).sort().join(',') !== 'bytes,path,sha256' ||
      typeof pin.path !== 'string' || !path.isAbsolute(pin.path) ||
      !Number.isSafeInteger(pin.bytes) || pin.bytes < 0 || pin.bytes > cap ||
      !/^[0-9a-f]{64}$/.test(pin.sha256)) throw new Error('closed source pin required');
  const fd = io.openSync(pin.path, io.constants.O_RDONLY | io.constants.O_NOFOLLOW | io.constants.O_NONBLOCK);
  let first = null;
  try {
    const st = io.fstatSync(fd);
    if (!st.isFile() || st.size !== pin.bytes) throw new Error('source type/size');
    const raw = Buffer.alloc(pin.bytes);
    let offset = 0;
    for (let calls=0; offset < raw.length && calls < 8; calls++) {
      const n = io.readSync(fd, raw, offset, raw.length-offset, null);
      if (n <= 0) throw new Error('source read incomplete');
      offset += n;
    }
    if (offset !== raw.length || digest(raw) !== pin.sha256) throw new Error('source pin differs');
    return raw;
  } catch (error) {
    first = error;
    throw error;
  } finally {
    try {
      io.closeSync(fd);
    } catch (error) {
      if (!first) throw error;
      first.secondary = [{phase:'close',
        class:String(error.constructor?.name || 'Error').slice(0,64),
        errno:Number.isInteger(error.errno) && error.errno >= -4096 && error.errno <= 4096 ? error.errno : null}];
    }
  }
}

function directory(pin) {
  if (!pin || Object.keys(pin).sort().join(',') !== 'dev,ino,path' ||
      !path.isAbsolute(pin.path) || path.resolve(pin.path) !== pin.path) throw new Error('owned allocation required');
  const st = fs.lstatSync(pin.path);
  if (!st.isDirectory() || st.isSymbolicLink() || st.dev !== pin.dev || st.ino !== pin.ino ||
      (st.mode & 0o777) !== 0o700) throw new Error('allocation identity differs');
  return pin.path;
}

export async function main(argv=process.argv) {
  if (argv.length !== 5 || argv[2] !== '--config' || !path.isAbsolute(argv[4])) throw new Error('fixed entry argv');
  const source = process.env.C3_NATIVE_DISPATCH;
  if (!source || !path.isAbsolute(source)) throw new Error('owner dispatch missing');
  const st = fs.lstatSync(source);
  if (!st.isFile() || st.isSymbolicLink() || st.size > 49152) throw new Error('dispatch type/size');
  const cfg = JSON.parse(readPinned({path:source,bytes:st.size,sha256:process.env.C3_NATIVE_DISPATCH_SHA || ''},49152));
  if (Object.keys(cfg).sort().join(',') !== 'P,T,argv,bootstrap,integration,schema' ||
      cfg.schema !== 'c3.native.owner-dispatch.v1' ||
      JSON.stringify(cfg.argv) !== JSON.stringify(argv.slice(0))) throw new Error('actual dispatch command differs');
  const P = directory(cfg.P), T = directory(cfg.T);
  if (path.dirname(T) !== P || path.dirname(P) !== process.env.C3_TRIAL_BASE) throw new Error('allocation scope');
  readPinned(cfg.bootstrap);
  const bootstrap = await import(pathToFileURL(cfg.bootstrap.path).href);
  let parentUsed = false, transactionUsed = false;
  const handoffWriter = bootstrap.makeWriter(cfg.integration.root);
  const operatorHandoff = payload => {
    // Child-owned handoff only. Parent must collect and durably copy independently.
    handoffWriter.write('handoff.json',Buffer.from(JSON.stringify(payload)+'\n'));
    return {token:cfg.integration.token,retained:true,stop_before_next:true};
  };
  const loadDriver = async driverPath => {
    const driver = await import(pathToFileURL(driverPath).href);
    return {runDriver: integration => driver.runDriver({
      ...integration,
      allocateParent(prefix) {
        if (parentUsed || prefix !== path.join(process.env.C3_TRIAL_BASE,'v-')) throw new Error('parent allocation reuse/shape');
        parentUsed = true;
        return directory(cfg.P);
      },
      createAdapter(module, context) {
        if (!parentUsed) throw new Error('parent allocation missing');
        return module.makeAdapter(context, {
          spawn: childProcess.spawn,
          kill: process.kill.bind(process),
          integration: context.integration,
          fileIO: Object.freeze({...fs, mkdtempSync(prefix) {
            if (transactionUsed || prefix !== path.join(P,'c3-adapter-')) throw new Error('transaction allocation reuse/shape');
            transactionUsed = true;
            return directory(cfg.T);
          }})
        });
      }
    })};
  };
  return bootstrap.runBootstrap(cfg.integration,{operatorHandoff,loadDriver});
}


export function renderFailure(error) {
  const number=error?.errno;
  const first={phase:'entry',class:String(error?.constructor?.name || 'Error').slice(0,64),
    errno:Number.isInteger(number) && number >= -4096 && number <= 4096 ? number : null};
  const facts=Array.isArray(error?.secondary)?error.secondary:[];
  const secondary=[];
  let omitted=Math.max(0,facts.length-4);
  for(const fact of facts.slice(0,4)) {
    if(fact && fact.phase==='close' && typeof fact.class==='string' &&
       (fact.errno===null || Number.isInteger(fact.errno) && fact.errno >= -4096 && fact.errno <= 4096)) {
      secondary.push({phase:'close',class:fact.class.slice(0,64),errno:fact.errno});
    } else omitted++;
  }
  // No message, pathname, raw error graph or fabricated original exception.
  return JSON.stringify({schema:'c3.native.owner-refusal.v1',first,secondary,omitted})+'\n';
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(new URL(import.meta.url).pathname)) {
  main().catch(error => {
    process.stderr.write(renderFailure(error));
    process.exitCode=4;
  });
}
