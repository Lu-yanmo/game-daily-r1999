"""gdagent.engine 离线自检(M2)：镜像 tools\\test-agent.ps1 的 a1–a7。

跑法(tools 目录下)： python gdagent/tests/test_agent.py
                   python -m unittest discover -s gdagent/tests -t .
约定同全家：不联网、不读真 config、不起真子进程 —— 写盘只发生在临时沙盒；
模型/verb/快照/确认全部走 send_exchange 的注入缝，一次网都不联。
"""
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))

from gdagent import engine as EN                       # noqa: E402
from gdagent import live as LV                         # noqa: E402

fake_spec_parser = EN.fake_spec_parser                  # 对拍/自检同一件：家只有一个（engine.py 顶部）


# ---------- [a1] set-config 白名单与取值校验 ----------

class TestConfigGate(unittest.TestCase):
    def test_kinds_table(self):
        self.assertEqual(EN.CONFIG_KINDS.get('ark.farmTargets'), 'farmTargets')
        self.assertEqual(EN.CONFIG_KINDS.get('ark.farmAppend'), 'bool')
        self.assertEqual(EN.CONFIG_KINDS.get('runtime.supervisor'), 'bool')
        self.assertGreaterEqual(len(EN.CONFIG_KINDS), 5)

    def test_path_whitelist(self):
        bad = EN.test_action({'type': 'set-config', 'path': 'schedule.daily', 'value': 'x'})
        self.assertFalse(bad['Ok'])
        self.assertIn('schedule.daily', bad['Why'])

    def test_string_true_normalizes_to_bool(self):
        r = EN.test_action({'type': 'set-config', 'path': 'ark.farmAppend', 'value': 'true'})
        self.assertTrue(r['Ok'])
        self.assertIs(r['Value'], True)

    def test_weekday_plan(self):
        wp = EN.check_config_value('weekdayPlan', {'1': 'CE-5', '8': 'LS-5'})
        self.assertFalse(wp['Ok'])
        wp2 = EN.check_config_value('weekdayPlan', {'6': 'CE-5,LS-5'})
        self.assertTrue(wp2['Ok'])
        self.assertEqual(wp2['Value']['6'], 'CE-5,LS-5')

    def test_max_times(self):
        self.assertFalse(EN.check_config_value('maxTimes', 0)['Ok'])
        self.assertFalse(EN.check_config_value('maxTimes', 31)['Ok'])
        r = EN.check_config_value('maxTimes', '12')
        self.assertTrue(r['Ok'])
        self.assertEqual(r['Value'], 12)

    def test_stage_list(self):
        known = {'CE-5': True}
        st1 = EN.check_config_value('stageList', 'CE-5,LS-5', known)
        self.assertFalse(st1['Ok'])
        self.assertIn('LS-5', st1['Why'])
        self.assertFalse(EN.check_config_value('stageList', 'CE-5!!', known)['Ok'])

    def test_farm_targets(self):
        empty = EN.check_config_value('farmTargets', None, spec_parser=fake_spec_parser)
        self.assertTrue(empty['Ok'])
        self.assertEqual(empty['Value'], [])
        good = EN.check_config_value('farmTargets', ['特种芯片组=8@PR-D-2'], spec_parser=fake_spec_parser)
        self.assertTrue(good['Ok'])
        bad = EN.check_config_value('farmTargets', ['ok一行', '坏行#bad'], spec_parser=fake_spec_parser)
        self.assertFalse(bad['Ok'])
        self.assertIn('坏行#bad', bad['Why'])            # 消息嵌的是解析器 Raw
        hole = EN.check_config_value('farmTargets', ['a=1', '   '], spec_parser=fake_spec_parser)
        self.assertFalse(hole['Ok'])
        self.assertIn('空条目', hole['Why'])
        no_parser = EN.check_config_value('farmTargets', ['a=1'], spec_parser=None)
        self.assertFalse(no_parser['Ok'])
        self.assertIn('ark-farm', no_parser['Why'])

    def test_unknown_action_type(self):
        r = EN.test_action({'type': 'reboot-mumu', 'reason': '模型突发奇想'})
        self.assertFalse(r['Ok'])


# ---------- [a2] run-verb 白名单 ----------

class TestVerbGate(unittest.TestCase):
    def test_over_gate(self):
        self.assertFalse(EN.check_verb_args(['-Force'])['Ok'])

    def test_dry_is_not_mutating(self):
        r = EN.check_verb_args(['-ArkStage', 'CE-5', '-DryRun'])
        self.assertTrue(r['Ok'])
        self.assertTrue(r['DryRun'])
        self.assertFalse(r['Mutating'])                  # 曾被误判成真跑的那条

    def test_no_dry_is_mutating(self):
        r = EN.check_verb_args(['-ArkStage', 'CE-5'])
        self.assertTrue(r['Ok'])
        self.assertTrue(r['Mutating'])

    def test_dup_and_value_rules(self):
        self.assertFalse(EN.check_verb_args(['-DryRun', '-DryRun'])['Ok'])
        self.assertFalse(EN.check_verb_args(['-Games'])['Ok'])
        self.assertFalse(EN.check_verb_args(['-Games', 'zzz'])['Ok'])

    def test_max_args(self):
        many = ['-DryRun', '-AckArk', '-Ack1999', '-ArkShopDone', '-RShopDone',
                '-ArkShopReset', '-RShopReset', '-FarmArk', '-Games', 'ark']
        r = EN.check_verb_args(many)
        self.assertFalse(r['Ok'])
        self.assertIn('最多', r['Why'])


# ---------- [a3] 模型回复解析 ----------

class TestReplyParse(unittest.TestCase):
    def test_fence_and_chatter(self):
        p1 = EN.resolve_reply('好的。```json\n{"reply":"收到","actions":[{"type":"preview"}]}\n```。')
        self.assertTrue(p1['Ok'])
        self.assertEqual(p1['Reply'], '收到')
        self.assertEqual(len(p1['Actions']), 1)
        p2 = EN.resolve_reply('今天天气不错，自己去玩吧')
        self.assertFalse(p2['Ok'])
        self.assertIn('天气', p2['Reply'])               # 纯聊天：原话照抄、零动作
        self.assertEqual(p2['Actions'], [])

    def test_broken_json_does_not_throw(self):
        p3 = EN.resolve_reply('{"reply":"半截","actions":[')
        self.assertFalse(p3['Ok'])
        self.assertIn('JSON', p3['Why'])

    def test_truncate_at_four(self):
        six = '{"reply":"多动作","actions":[' + ','.join(['{"type":"preview"}'] * 6) + ']}'
        p4 = EN.resolve_reply(six)
        self.assertEqual(len(p4['Actions']), 4)
        self.assertIn('截', p4['Why'])


# ---------- [a4] 多轮消息组装 ----------

class TestMessages(unittest.TestCase):
    def test_order_and_content(self):
        hist = [{'role': 'user', 'text': 'u1'}, {'role': 'assistant', 'text': 'a1'}]
        m1 = EN.build_messages('BOOK', hist, 'SNAP', 'NOW')
        self.assertEqual(len(m1), 4)
        self.assertEqual(m1[0], {'role': 'system', 'content': 'BOOK'})
        self.assertEqual(m1[3]['role'], 'user')
        self.assertIn('SNAP', m1[3]['content'])
        self.assertIn('NOW', m1[3]['content'])

    def test_history_cap(self):
        fat = [{'role': 'user', 'text': 'm%d' % i} for i in range(1, 31)]
        m2 = EN.build_messages('B', fat, '', 'NOW', max_turns=2)
        self.assertEqual(len(m2), 6)                     # system + 最近 4 条 + 本轮 user
        self.assertEqual(len(EN.build_messages('B', [], '', 'Q')), 2)


# ---------- [a5] journal 回滚计划 ----------

class TestRollbackPlan(unittest.TestCase):
    def test_only_set_reversed(self):
        lines = [
            '{"ts":"2026-09-26 10:00:00","op":"set","path":"ark.farmAppend","old":true,"new":false,"backup":""}',
            '这不是 JSON(半行/人工注释)，回滚计划必须跳过它',
            '{"ts":"2026-09-26 10:01:00","op":"rollback","items":2}',
            '{"ts":"2026-09-26 10:02:00","op":"Set","path":"ark.defaultStage","old":"CE-5","new":"LS-5","backup":""}',
        ]
        plan = EN.rollback_plan(lines)
        self.assertEqual(len(plan), 2)                   # -eq 不分大小写：'Set' 也算
        self.assertEqual(plan[0]['path'], 'ark.defaultStage')   # 最新的先恢复


# ---------- [a6] 活层 apply→journal→rollback(临时沙盒) ----------

class TestSandboxLive(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix='gd-agent-test-')
        self.paths = EN.Paths(self.dir)
        self.paths.settings.parent.mkdir(parents=True, exist_ok=True)
        self._write('{"ark":{"farmAppend":true,"farmMaxTimesPerRound":12},"runtime":{"supervisor":true}}')

    def tearDown(self):
        import shutil
        shutil.rmtree(self.dir, ignore_errors=True)

    def _write(self, text):
        self.paths.settings.write_text(text, encoding='utf-8')

    def _read(self):
        return EN.read_json(self.paths.settings)

    def test_apply_journal_rollback(self):
        norm = EN.test_action({'type': 'set-config', 'path': 'runtime.supervisor', 'value': False},
                              spec_parser=fake_spec_parser)
        self.assertTrue(norm['Ok'])
        ap = EN.apply_set_config(self.paths, norm)
        self.assertTrue(ap['Ok'])
        self.assertIn('回读一致=True', ap['Detail'])
        rb = EN.test_action({'type': 'set-config', 'path': 'ark.farmMaxTimesPerRound', 'value': 8})
        ap2 = EN.apply_set_config(self.paths, rb, backup_done=True)
        self.assertTrue(ap2['Ok'])
        self.assertTrue(self.paths.journal.is_file())
        jlines = [ln for ln in self.paths.journal.read_text(encoding='utf-8').splitlines() if ln.strip()]
        self.assertEqual(len(jlines), 2)
        self.assertEqual(len(list(self.paths.backups.glob('settings.json*.bak'))), 1)   # 第一条才备份
        dry = EN.rollback_settings(self.paths, dry=True)
        self.assertIn('待回滚 2 项', dry)
        self.assertIs(self._read()['runtime']['supervisor'], False)   # -Dry 只报数，一个字不改
        txt = EN.rollback_settings(self.paths)
        self.assertIn('已回滚 2 项', txt)
        back = self._read()
        self.assertIs(back['runtime']['supervisor'], True)
        self.assertEqual(back['ark']['farmMaxTimesPerRound'], 12)
        raw = self.paths.journal.read_text(encoding='utf-8')
        self.assertIn('"op":"rollback"', raw)            # compact 形状是 rollback 标记失配事故的命门
        self.assertIn('没有变更', EN.rollback_settings(self.paths))

    def test_missing_settings(self):
        p2 = EN.Paths(os.path.join(self.dir, 'nowhere'))
        r = EN.apply_set_config(p2, {'Path': 'ark.farmAppend', 'Value': False})
        self.assertFalse(r['Ok'])
        self.assertIn('settings.json', r['Detail'])


# ---------- [a7] 会话循环：假模型喂动作、真跑必须过确认钩子 ----------

class TestExchangeLoop(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix='gd-agent-loop-')
        self.paths = EN.Paths(self.dir)
        self.paths.settings.parent.mkdir(parents=True, exist_ok=True)
        self.paths.settings.write_text('{"ark":{"farmAppend":true},"runtime":{"supervisor":true}}', encoding='utf-8')
        self.verb_calls = []
        self.model_calls = []
        self.queue = [
            '{"reply":"先关监督并把预览跑一下","actions":['
            '{"type":"set-config","path":"runtime.supervisor","value":false},'
            '{"type":"preview"},'
            '{"type":"run-verb","args":["-FarmArk"]},'
            '{"type":"set-config","path":"schedule.daily","value":"x"}]}',
            '{"reply":"都办完了","actions":[]}',
        ]

    def tearDown(self):
        import shutil
        shutil.rmtree(self.dir, ignore_errors=True)

    def _model(self, msgs):
        self.model_calls.append(msgs)
        if self.queue:
            return {'Ok': True, 'Text': self.queue.pop(0)}
        return {'Ok': False, 'Error': '自检钉死：不该再问第 %d 次' % (len(self.model_calls) + 1)}

    def _verb(self, args, allowed=False):
        self.verb_calls.append({'args': list(args), 'allowed': allowed})
        return {'Ran': True, 'Exit': 0, 'Detail': '假执行：' + ' '.join(args)}

    def test_one_exchange_two_rounds(self):
        history = []
        confirm_asked = []
        final = EN.send_exchange(self.paths, '关掉监督，预览一下，再试材料轮', history,
                                 self._model, self._verb,
                                 known_stages={}, spec_parser=fake_spec_parser,
                                 confirm_fn=lambda d: confirm_asked.append(d) or False,
                                 snapshot_fn=lambda: 'SNAP', printer=lambda t: None)
        self.assertEqual(len(self.model_calls), 2)       # 第二次带回执，没超 MaxRounds 空转
        second = '\n'.join(m['content'] for m in self.model_calls[1])
        self.assertIn('schedule.daily', second)          # 白名单外动作进了回执
        self.assertIn('不在白名单', second)
        dry_calls = [c for c in self.verb_calls if '-DryRun' in c['args']]
        self.assertEqual(len(dry_calls), 1)              # preview 自动放行
        self.assertEqual([c for c in self.verb_calls if '-DryRun' not in c['args']], [])
        self.assertTrue(any('FarmArk' in d for d in confirm_asked))   # 真跑走了确认钩子
        self.assertIn('都办完', final)
        self.assertEqual(history[0]['role'], 'user')
        self.assertIn('关掉监督', history[0]['text'])
        self.assertGreaterEqual(len(history), 4)
        self.assertIs(self._supervisor(), False)         # 自动应用档生效(没等人确认)

    def _supervisor(self):
        return EN.read_json(self.paths.settings)['runtime']['supervisor']

    def test_model_failure_breaks_cleanly(self):
        history = []
        final = EN.send_exchange(self.paths, '随便', history,
                                 lambda m: {'Ok': False, 'Error': '没网'}, lambda a, allowed=False: None,
                                 printer=lambda t: None, snapshot_fn=lambda: '')
        self.assertIn('问模型没成', final)


# ---------- M3：kb-search 第四动作(闸/包装/频控/去重/回路) ----------

class TestKbSearchGate(unittest.TestCase):
    def test_accepts_and_defaults(self):
        n = EN.test_action({'type': 'kb-search', 'query': '密合值满了会怎样', 'top': 2})
        self.assertTrue(n['Ok'])
        self.assertEqual(n['Query'], '密合值满了会怎样')
        self.assertEqual(n['Top'], 2)
        n = EN.test_action({'type': 'KB-Search', 'query': '  共鸣 触发 '})   # type 大小写不敏感同 PS
        self.assertTrue(n['Ok'])
        self.assertEqual(n['Top'], EN.KB_TOP_DEFAULT)
        self.assertEqual(n['Query'], '共鸣 触发')                              # 首尾空白剥掉

    def test_game_field_ignored(self):
        n = EN.test_action({'type': 'kb-search', 'query': 'x', 'game': 'ark'})  # 模型不许填 game：填了也只是忽略
        self.assertTrue(n['Ok'])
        self.assertNotIn('Game', n)

    def test_rejects(self):
        for act in ({'type': 'kb-search'},
                    {'type': 'kb-search', 'query': '   '},
                    {'type': 'kb-search', 'query': '长' * 201},
                    {'type': 'kb-search', 'query': 'x', 'top': 0},
                    {'type': 'kb-search', 'query': 'x', 'top': 6},
                    {'type': 'kb-search', 'query': 'x', 'top': '很多'}):
            self.assertFalse(EN.test_action(act)['Ok'], act)


class TestKbReceipt(unittest.TestCase):
    def _hit(self, i, text='正文'):
        return {'id': 'x%d' % i, 'game': 'r1999', 'title': '条目%d' % i, 'text': text,
                'score': 0.5, 'source': 'src%d' % i, 'updated': '2026-09-27'}

    def test_no_hits_wording(self):
        r = EN.format_kb_receipt({'ok': True, 'hits': [], 'why': '没找到'})
        self.assertIn('no_hits', r)
        self.assertIn('禁止编', r)

    def test_failure_wording(self):
        self.assertIn('知识库检索没成', EN.format_kb_receipt({'ok': False, 'hits': [], 'why': '模型不符'}))
        self.assertIn('知识库检索没成', EN.format_kb_receipt(None))

    def test_wrap_header_and_citations(self):
        r = EN.format_kb_receipt({'ok': True, 'hits': [self._hit(1)], 'why': ''})
        self.assertTrue(r.startswith(EN.KB_RECEIPT_HEADER))
        self.assertIn('出处：src1', r)
        self.assertIn('知识截至 2026-09-27', r)

    def test_per_block_truncation(self):
        r = EN.format_kb_receipt({'ok': True, 'hits': [self._hit(1, '字' * 2000)], 'why': ''},
                                 budget=100000)
        self.assertIn('块已截断', r)
        self.assertNotIn('字' * (EN.KB_PER_BLOCK + 1), r)

    def test_budget_drops_tail(self):
        hits = [self._hit(i, '字' * 800) for i in range(1, 6)]
        r = EN.format_kb_receipt({'ok': True, 'hits': hits, 'why': ''}, budget=1500)
        self.assertIn('回执预算被省略', r)
        self.assertIn('[1]', r)
        self.assertNotIn('[5]', r)


class TestKbExchange(unittest.TestCase):
    """回路：模型提 kb-search → 引擎进程内检索 → 包装回执进下一轮 → 频控/去重话术。"""

    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix='gd-agent-kb-')
        self.paths = EN.Paths(self.dir)
        self.paths.settings.parent.mkdir(parents=True, exist_ok=True)
        self.paths.settings.write_text('{}', encoding='utf-8')
        self.kb_calls = []

    def tearDown(self):
        import shutil
        shutil.rmtree(self.dir, ignore_errors=True)

    def _kb(self, query, top):
        self.kb_calls.append((query, top))
        return {'ok': True, 'hits': [{'id': 'a', 'game': 'r1999', 'title': 'T', 'text': query + '的正文',
                                      'score': 0.6, 'source': 's', 'updated': '2026-09-01'}],
                'rerank': True, 'why': ''}

    def _run(self, queue):
        holder = {'i': 0}

        def model(msgs):
            txt = queue[holder['i']] if holder['i'] < len(queue) else '{"reply":"完","actions":[]}'
            holder['i'] += 1
            return {'Ok': True, 'Text': txt}

        history = []
        state = {'seen': set()}
        EN.send_exchange(self.paths, '查点知识', history, model, lambda a, allowed=False: None,
                         snapshot_fn=lambda: '', printer=lambda t: None,
                         kb_fn=self._kb, kb_state=state)
        return history, state

    @staticmethod
    def _acts(q):
        return '{"reply":"查","actions":[{"type":"kb-search","query":"%s"}]}' % q

    def test_receipt_wrapped_and_passed_to_next_round(self):
        history, _ = self._run([self._acts('密合 机制'), '{"reply":"根据资料作答","actions":[]}'])
        joined = '\n'.join(h['text'] for h in history)
        self.assertIn(EN.KB_RECEIPT_HEADER, joined)          # 外层包装进了回执与历史
        self.assertIn('出处：s', joined)

    def test_dedup_same_query_within_session(self):
        # 第 2 轮拿空白变体重查第 1 轮已查过的内容：吃"已检索过"，且不占频控预算
        queue = [self._acts('共鸣'), self._acts(' 共 鸣 '), self._acts('密合'),
                 '{"reply":"完","actions":[]}']
        history, _ = self._run(queue)
        joined = '\n'.join(h['text'] for h in history)
        self.assertIn('已检索过', joined)
        self.assertEqual([q for q, _ in self.kb_calls], ['共鸣', '密合'])   # 真检索只发生了两次

    def test_round_cap_two(self):
        # 同一份模型回复里排四条 kb-search：前两条真检索，后两条吃封顶话术
        queue = ['{"reply":"查","actions":[{"type":"kb-search","query":"a"},'
                 '{"type":"kb-search","query":"b"},{"type":"kb-search","query":"c"},'
                 '{"type":"kb-search","query":"d"}]}',
                 '{"reply":"完","actions":[]}']
        history, state = self._run(queue)
        joined = '\n'.join(h['text'] for h in history)
        self.assertIn('本轮已经检索过两次', joined)
        self.assertEqual(len(self.kb_calls), 2)
        self.assertEqual(state['round_used'], 2)

    def test_kb_fn_exception_becomes_receipt_not_crash(self):
        def bad(query, top):
            raise RuntimeError('索引炸了')
        queue = [self._acts('x'), '{"reply":"收到","actions":[]}']

        def model(msgs):
            return {'Ok': True, 'Text': queue.pop(0) if queue else '{"reply":"完","actions":[]}'}

        history = []
        EN.send_exchange(self.paths, '查', history, model,
                         lambda a, allowed=False: None, snapshot_fn=lambda: '',
                         printer=lambda t: None, kb_fn=bad)
        joined = '\n'.join(h['text'] for h in history)
        self.assertIn('检索出了意外', joined)
        self.assertIn('索引炸了', joined)

    def test_no_kb_fn_engine_says_not_connected(self):
        history = []
        EN.send_exchange(self.paths, '查', history,
                         lambda m: {'Ok': True, 'Text': self._acts('x')},
                         lambda a, allowed=False: None, snapshot_fn=lambda: '',
                         printer=lambda t: None)          # 不接 kb_fn 的旧装配(对拍/训练)：动作仍过闸，回执说没接
        joined = '\n'.join(h['text'] for h in history)
        self.assertIn('没接 kb_fn', joined)


# ---------- 活层里可离线钉的两件：降级快照 / 假件装配 ----------

class TestLiveOffline(unittest.TestCase):
    def test_degraded_snapshot_shape(self):
        d = tempfile.mkdtemp(prefix='gd-live-snap-')
        try:
            paths = EN.Paths(d)
            paths.agent_dir.mkdir(parents=True)
            paths.journal.write_text('{"ts":"2026-09-27 10:00:00","op":"set","path":"a.b","old":1,"new":2,"backup":""}\n',
                                     encoding='utf-8')
            txt = LV.degraded_snapshot(paths, {'ark': {'farmTargets': ['X=1@Y'], 'farmAppend': True}})
            self.assertIn('配置：', txt)
            self.assertIn('补刷判表：快照助手没应答', txt)   # 缺什么就明说什么缺
            self.assertIn('最近留底：', txt)
            self.assertNotIn('"ts":', txt.split('最近留底：')[1])
        finally:
            import shutil
            shutil.rmtree(d, ignore_errors=True)

    def test_verb_line_filter(self):
        self.assertTrue(LV.VERB_LINE.search('[STEP] 开始'))
        self.assertTrue(LV.VERB_LINE.search('结果：一切正常'))
        self.assertIsNone(LV.VERB_LINE.search('一行普通日志'))
        self.assertEqual(LV.VERB_LEAD.sub('', '[WARN] 理智不足'), ' 理智不足')


if __name__ == '__main__':
    unittest.main(verbosity=2)
