"""Validate owner-operated desktop trials; never executes a desktop action."""
import argparse
import json
import math
from pathlib import Path

ROOT=Path(__file__).resolve().parent.parent
MATRIX=ROOT/'docs/testing/desktop-control/matrix.json'
OUTCOMES={'verified','unconfirmed','blocked','stopped','failed','unavailable'}

def wilson(success,total):
    if not total:return None
    z=1.95996398454;p=success/total;d=1+z*z/total
    midpoint=(p+z*z/(2*total))/d
    half=z*math.sqrt(p*(1-p)/total+z*z/(4*total*total))/d
    return [max(0,midpoint-half),min(1,midpoint+half)]

def report(matrix,rows):
    core={c['id']:c for c in matrix['core']};held={c['id']:c for c in matrix['held_out']}
    if len(core)!=30 or len(held)!=20:raise ValueError('fixed qualification matrix must contain 30 core and 20 held-out cases')
    samples=set();slots=set();sources=set();builds=set();counts={'core':0,'held_out':0,'voice':0};success=dict.fromkeys(counts,0)
    bad={'wrong_target_actions':0,'unauthorized_actions':0,'dispatches_after_cancel':0};latencies=[];outcomes={k:0 for k in OUTCOMES}
    for r in rows:
        if not isinstance(r,dict):raise ValueError('trial must be an object')
        sid=r.get('sample_id');split=r.get('split');case=r.get('case_id');attempt=r.get('attempt')
        if not isinstance(sid,str)or not sid or sid in samples:raise ValueError('sample IDs must be unique and nonempty')
        samples.add(sid)
        if split not in counts:raise ValueError('unknown trial split')
        allowed=held if split=='held_out' else set(core)
        limit=10 if split=='core' else 1
        if case not in allowed or type(attempt)is not int or not 1<=attempt<=limit:raise ValueError('invalid case or attempt')
        slot=(split,case,attempt)
        if slot in slots:raise ValueError('duplicate case attempt')
        slots.add(slot)
        for key in ('source_sha256','build_identity','app_version','display_configuration','evidence_ref'):
            if not isinstance(r.get(key),str)or not r[key].strip():raise ValueError('missing provenance: '+key)
        source=r['source_sha256']
        if len(source)!=64 or any(c not in '0123456789abcdef' for c in source):raise ValueError('invalid source fingerprint')
        if r.get('platform')!='windows' or r.get('input_mode')!=('voice' if split=='voice' else 'text'):raise ValueError('platform or input mode mismatch')
        if r.get('failure_stage') not in {'none','speech','interpretation','target_resolution','dispatch','effect_verification'}:raise ValueError('invalid failure stage')
        if split=='held_out' and type(r.get('expectation_met'))is not bool:raise ValueError('held-out result requires explicit expectation assessment')
        if r.get('outcome')not in OUTCOMES:raise ValueError('unknown outcome')
        if type(r.get('effect_observed'))is not bool:raise ValueError('effect evidence must be boolean')
        for key in bad:
            if type(r.get(key))is not int or r[key]<0:raise ValueError('invalid safety count: '+key)
            bad[key]+=r[key]
        latency=r.get('latency_ms')
        if type(latency)not in (int,float)or not math.isfinite(latency)or latency<0:raise ValueError('invalid latency')
        sources.add(source);builds.add(r['build_identity']);latencies.append(latency);counts[split]+=1;outcomes[r['outcome']]+=1
        success[split]+=int((r['expectation_met'] and r['outcome'] in held[case]['expected_outcomes'] and (r['outcome']!='verified' or r['effect_observed'])) if split=='held_out' else (r['outcome']=='verified'and r['effect_observed']))
    if len(sources)>1 or len(builds)>1:raise ValueError('assess one exact source/build at a time')
    complete=counts=={'core':300,'held_out':20,'voice':30}
    metrics_met=complete and success['core']>=297 and success['held_out']==20 and success['voice']==30 and not any(bad.values())
    return {'status':'metrics_met_device_signoff_pending'if metrics_met else 'in_progress',
            'production_ready':False,'counts':counts,'required_counts':{'core':300,'held_out':20,'voice':30},
            'successful_cases':success,'outcomes':outcomes,'safety_counts':bad,
            'core_observed_completion':success['core']/counts['core']if counts['core']else None,
            'core_wilson_95':wilson(success['core'],counts['core']),
            'median_latency_ms':sorted(latencies)[len(latencies)//2]if latencies else None,
            'source_sha256':next(iter(sources),None),'build_identity':next(iter(builds),None),
            'remaining_gates':['full_native_library_and_integration','release_provenance','owner_device_signoff','weighted_readiness_score_and_floors']}

def main():
    p=argparse.ArgumentParser();p.add_argument('--results',type=Path);p.add_argument('--output',type=Path);args=p.parse_args()
    matrix=json.loads(MATRIX.read_text(encoding='utf-8'))
    rows=json.loads(args.results.read_text(encoding='utf-8'))if args.results else []
    if not isinstance(rows,list):raise ValueError('results must be a JSON array')
    data=report(matrix,rows);text=json.dumps(data,indent=2)+'\n'
    if args.output:args.output.write_text(text,encoding='utf-8')
    else:print(text,end='')
if __name__=='__main__':main()
