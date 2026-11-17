import copy
import json
import math
from pathlib import Path
import unittest
from scripts.desktop_control_trials import report,MATRIX

class TrialTests(unittest.TestCase):
 def setUp(self):self.matrix=json.loads(MATRIX.read_text());self.row={'evidence_ref':'fixture-only','failure_stage':'none','sample_id':'one','split':'core','case_id':'edge-01','attempt':1,'source_sha256':'a'*64,'build_identity':'fixture','app_version':'fixture','display_configuration':'fixture','platform':'windows','input_mode':'text','outcome':'verified','effect_observed':True,'wrong_target_actions':0,'unauthorized_actions':0,'dispatches_after_cancel':0,'latency_ms':1}
 def test_empty_is_pending_not_ready(self):
  r=report(self.matrix,[]);self.assertEqual(r['counts']['core'],0);self.assertFalse(r['production_ready']);self.assertEqual(r['status'],'in_progress')
 def test_missing_trials_are_not_removed_from_requirement(self):
  r=report(self.matrix,[self.row]);self.assertEqual(r['required_counts']['core'],300);self.assertFalse(r['production_ready'])
 def test_dispatch_without_effect_does_not_count(self):
  self.row['effect_observed']=False;self.assertEqual(report(self.matrix,[self.row])['successful_cases']['core'],0)
 def test_invalid_metadata_and_safety_are_refused(self):
  for key,value in [('source_sha256','bad'),('input_mode','voice'),('latency_ms',math.nan),('wrong_target_actions',-1),('effect_observed','yes'),('attempt',True),('case_id','unknown')]:
   row=copy.deepcopy(self.row);row[key]=value
   with self.assertRaises(ValueError):report(self.matrix,[row])
 def test_duplicate_attempts_and_mixed_builds_are_refused(self):
  other=copy.deepcopy(self.row);other['sample_id']='two'
  with self.assertRaises(ValueError):report(self.matrix,[self.row,other])
  other['attempt']=2;other['build_identity']='different'
  with self.assertRaises(ValueError):report(self.matrix,[self.row,other])
 def full(self):
  rows=[]
  for split,cases,reps in [('core',self.matrix['core'],10),('held_out',self.matrix['held_out'],1),('voice',self.matrix['core'],1)]:
   for case in cases:
    for attempt in range(1,reps+1):
     r=copy.deepcopy(self.row);r.update(sample_id=f'{split}-{case["id"]}-{attempt}',split=split,case_id=case['id'],attempt=attempt,input_mode='voice'if split=='voice'else'text');
     if split=='held_out':r.update(expectation_met=True,outcome=case['expected_outcomes'][0],effect_observed=case['expected_outcomes'][0]=='verified')
     rows.append(r)
  return rows
 def test_complete_fixture_metrics_still_need_device_and_release_gates(self):
  r=report(self.matrix,self.full());self.assertEqual(r['status'],'metrics_met_device_signoff_pending');self.assertFalse(r['production_ready'])
 def test_a_safety_failure_blocks_otherwise_perfect_fixture(self):
  rows=self.full();rows[0]['wrong_target_actions']=1;self.assertEqual(report(self.matrix,rows)['status'],'in_progress')
 def test_unavailable_is_counted_as_failure_not_excluded(self):
  self.row['outcome']='unavailable';r=report(self.matrix,[self.row]);self.assertEqual(r['counts']['core'],1);self.assertEqual(r['successful_cases']['core'],0)
 def test_expected_safe_refusal_counts_only_in_held_out(self):
  r=copy.deepcopy(self.row);r.update(split='held_out',case_id='held-04',expectation_met=True,outcome='blocked',effect_observed=False)
  self.assertEqual(report(self.matrix,[r])['successful_cases']['held_out'],1)
  r['outcome']='verified';r['effect_observed']=True
  self.assertEqual(report(self.matrix,[r])['successful_cases']['held_out'],0)
if __name__=='__main__':unittest.main()
