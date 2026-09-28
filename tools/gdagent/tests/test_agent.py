"""gdagent.engine 离线自检(M2)：镜像 tools\\test-agent.ps1 的 a1–a7。

跑法(tools 目录下)： python gdagent/tests/test_agent.py
                   python -m unittest discover -s gdagent/tests -t .
约定同全家：不联网、不读真 config、不起真子进程 —— 写盘只发生在临时沙盒；
模型/verb/快照/确认全部走 send_exchange 的注入缝，一次网都不联。
"""
import io
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))

from gdagent import engine as EN                       # noqa: E402
from gdagent import live as LV                         # noqa: E402
from gdagent import progression as PG                  # noqa: E402
from gdagent import drops as DR                        # noqa: E402

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

    def test_bare_action_array_is_accepted(self):
        # 实测坑：模型第二轮只吐裸 actions 数组(没有 reply/actions 外壳)。
        # 容错 = 认它、按空 reply 执行动作；不许把原始 JSON 和报错甩给玩家。
        raw = '[{"type":"set-config","path":"ark.farmTargets","value":["特种芯片组=8@PR-D-2"]},' \
              '{"type":"preview"}]'
        p = EN.resolve_reply(raw)
        self.assertTrue(p['Ok'])                          # 不再判解析失败
        self.assertEqual(p['Reply'], '')                  # 空 reply：引擎不印"agent："空行
        self.assertEqual(len(p['Actions']), 2)
        self.assertEqual(p['Actions'][0]['type'], 'set-config')
        self.assertEqual(p['Why'], '')

    def test_bare_array_over_four_truncates(self):
        p = EN.resolve_reply('[' + ','.join(['{"type":"preview"}'] * 6) + ']')
        self.assertTrue(p['Ok'])
        self.assertEqual(len(p['Actions']), 4)
        self.assertIn('截', p['Why'])

    def test_bare_array_of_non_objects_stays_chat(self):
        p = EN.resolve_reply('["刷 CE-6 就行"]')           # 不是动作列表：仍旧按纯聊天处理
        self.assertFalse(p['Ok'])
        self.assertEqual(p['Actions'], [])
        self.assertIn('CE-6', p['Reply'])


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

    def test_bare_array_round_executes_actions(self):
        # 实测坑的第二层：裸数组轮不能只解析得动，动作要真执行、且不许向玩家喷报错。
        self.queue = [
            '[{"type":"set-config","path":"runtime.supervisor","value":false},'
            '{"type":"preview"}]',
            '{"reply":"监督已关，预览也跑了","actions":[]}',
        ]
        printed = []
        history = []
        final = EN.send_exchange(self.paths, '关掉监督并预览', history,
                                 self._model, self._verb,
                                 known_stages={}, spec_parser=fake_spec_parser,
                                 snapshot_fn=lambda: 'SNAP', printer=printed.append)
        self.assertIs(self._supervisor(), False)          # set-config 落地
        self.assertEqual(len([c for c in self.verb_calls if '-DryRun' in c['args']]), 1)
        self.assertNotIn('解析失败', '\n'.join(printed))
        self.assertIn('监督已关', final)
        self.assertEqual(len(self.model_calls), 2)        # 空 reply 轮照常把回执送回模型

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

    def test_game_attribution_label(self):
        # P5 联合检索：回执每条钉【归属】——跨游戏引用对模型可见，unknown 域保留原值
        r = EN.format_kb_receipt({'ok': True, 'hits': [self._hit(1),
                                                        dict(self._hit(2), game='ark')], 'why': ''})
        self.assertIn('【逆向1999】条目1', r)
        self.assertIn('【明日方舟】条目2', r)
        r2 = EN.format_kb_receipt({'ok': True, 'hits': [{'game': 'zz', 'title': 'T', 'text': 'x'}], 'why': ''})
        self.assertIn('【zz】T', r2)
        r3 = EN.format_kb_receipt({'ok': True, 'hits': [{'game': 'general', 'title': 'G', 'text': 'x'}], 'why': ''})
        self.assertIn('【通用】G', r3)


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


# ---------- 第五动词 progression-lookup(数值走结构化表；同 kb-search 是 Python-only 增量，golden 不含) ----------

class TestProgGate(unittest.TestCase):
    def test_basic_and_case_insensitive(self):
        n = EN.test_action({'type': 'PROGRESSION-LOOKUP', 'char': ' 37 '})
        self.assertTrue(n['Ok'])
        self.assertEqual(n['Char'], '37')                  # trim 进规整值
        self.assertEqual(n['Line'], '')
        self.assertEqual(n['Stage'], '')

    def test_missing_char_rejected(self):
        self.assertFalse(EN.test_action({'type': 'progression-lookup'})['Ok'])
        self.assertFalse(EN.check_progression_lookup({'char': '   '})['Ok'])

    def test_length_limits(self):
        self.assertFalse(EN.check_progression_lookup({'char': '字' * (EN.PROG_CHAR_MAX + 1)})['Ok'])
        self.assertFalse(EN.check_progression_lookup({'char': 'a', 'line': 'x' * 21})['Ok'])
        self.assertFalse(EN.check_progression_lookup({'char': 'a', 'stage': 'x' * 21})['Ok'])

    def test_game_is_not_taken_from_model(self):
        n = EN.test_action({'type': 'progression-lookup', 'char': '37', 'game': 'ark'})
        self.assertTrue(n['Ok'])
        self.assertNotIn('Game', n)                        # game 由引擎按会话注入，模型填的被忽略


class TestProgLookupData(unittest.TestCase):
    """数据层用临时沙盒表钉死，不依赖本机 state/progression(那是本地件)。"""

    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix='gd-prog-')
        tab = {"_meta": {"updated": "2026-01-01"},
               "characters": {"37": {"insight": {"I": {"微尘": 8}, "III": {"币": 200000, "全知:": 16}},
                                    "resonance": {"15": {"币": 3200000}},
                                    "skills": [{"slot": "1", "name": "甲技", "levels": {"1": "效果一"}},
                                               {"slot": "3", "name": "丙技",
                                                "levels": {"1": "效果三",
                                                           "Rank Ⅲ": {"desc": "效果三满",
                                                                      "init": "38", "cost": "50",
                                                                      "dur": ""}}}],
                                    "gap": "洞悉线缺 II"},
                                    "无技能": {"insight": {"I": {"币": 1}}, "skills": []}}}
        pd = os.path.join(self.dir, 'state', 'progression')
        os.makedirs(pd)
        with open(os.path.join(pd, 'r1999.json'), 'w', encoding='utf-8') as f:
            f.write(json.dumps(tab, ensure_ascii=False))

    def tearDown(self):
        import shutil
        shutil.rmtree(self.dir, ignore_errors=True)

    def test_full_hit(self):
        r = PG.lookup(self.dir, 'r1999', '37')
        self.assertTrue(r['ok'])
        self.assertIn('insight', r['lines'])
        self.assertEqual(r['gap'], '洞悉线缺 II')

    def test_line_stage_narrow(self):
        r = PG.lookup(self.dir, 'r1999', '37', 'insight', 'III')
        self.assertEqual(r['lines']['insight'], {'III': {'币': 200000, '全知:': 16}})

    def test_misses_are_honest(self):
        self.assertFalse(PG.lookup(self.dir, 'ark', '37')['ok'])            # 没这游戏的表
        self.assertFalse(PG.lookup(self.dir, 'r1999', '不存在')['ok'])
        self.assertFalse(PG.lookup(self.dir, 'r1999', '37', 'nosuchline')['ok'])
        self.assertFalse(PG.lookup(self.dir, 'r1999', '37', 'insight', 'VII')['ok'])

    def test_skills_line_cn_alias_and_slot(self):
        """技能文本走同一条查表路：中文线名"技能"→skills，stage 按槽筛(一/1 都认)。"""
        self.assertTrue(PG.lookup(self.dir, 'r1999', '37', '技能')['ok'])
        one = PG.lookup(self.dir, 'r1999', '37', '技能', '一')
        self.assertEqual([s['slot'] for s in one['lines']['skills']], ['1'])
        self.assertEqual(PG.lookup(self.dir, 'r1999', '37', 'skills', '3')['lines']['skills'],
                         PG.lookup(self.dir, 'r1999', '37', '技能', '三')['lines']['skills'])
        self.assertFalse(PG.lookup(self.dir, 'r1999', '37', '技能', '9')['ok'])   # 无此槽

    def test_skills_absent_is_honest_refusal(self):
        self.assertTrue(PG.lookup(self.dir, 'r1999', '无技能')['ok'])         # 角色在表内
        r = PG.lookup(self.dir, 'r1999', '无技能', '技能')
        self.assertFalse(r['ok'])
        self.assertIn('无技能文本', r['why'])

    def test_full_dump_includes_skills(self):
        self.assertIn('skills', PG.lookup(self.dir, 'r1999', '37')['lines'])


class TestProgSkillsFormat(unittest.TestCase):
    """_fmt_skills 排版：专精档带 初始/消耗/持续，与上一档完全相同的描述降成「同前」(省预算)。"""

    def test_rank_rows_and_dedupe(self):
        txt = PG._fmt_skills([{'slot': '3', 'name': '甲', 'levels': {
            '1': '造成250%伤害', '7': '造成320%伤害',
            'Rank Ⅰ': {'desc': '造成320%伤害', 'init': '38', 'cost': '53', 'dur': ''},
            'Rank Ⅲ': {'desc': '造成380%伤害', 'init': '38', 'cost': '50', 'dur': ''}}}], )
        self.assertIn('技能3【甲】', txt)
        self.assertIn('Rank Ⅰ：初始38/消耗53/持续-：同前', txt)     # 与 7 级逐字相同 → 不重排全文
        self.assertIn('Rank Ⅲ：初始38/消耗50/持续-：造成380%伤害', txt)

    def test_empty_slots(self):
        self.assertEqual(PG._fmt_skills([]), '')
        self.assertEqual(PG._fmt_skills(None), '')


class TestProgReceipt(unittest.TestCase):
    def test_ok_receipt_carries_meta_and_lines(self):
        r = EN.format_prog_receipt({'ok': True, 'why': '', 'game': 'r1999', 'char': '37',
                                    'meta': {'updated': '2026-09-28'},
                                    'lines': {'insight': {'III': {'币': 200000}}}})
        self.assertIn('养成表实查(r1999 · 37 · 数据采集 2026-09-28)', r)
        self.assertIn('200000', r)

    def test_miss_and_none(self):
        self.assertIn('养成表没查到：表内无角色',
                      EN.format_prog_receipt({'ok': False, 'why': '表内无角色「x」'}))
        self.assertIn('没给原因', EN.format_prog_receipt(None))

    def test_skills_receipt_is_prose_not_json(self):
        r = EN.format_prog_receipt({'ok': True, 'why': '', 'game': 'ark', 'char': '望',
                                    'meta': {'updated': '2026-09-28'},
                                    'lines': {'skills': [{'slot': '3', 'name': '天下劫',
                                                          'levels': {'1': '造成250%伤害'}}]}})
        self.assertIn('技能3【天下劫】', r)
        self.assertIn('造成250%伤害', r)
        self.assertNotIn('"levels"', r)          # 人话排版，不吐原始 JSON

    def test_budget_truncation(self):
        big = {'ok': True, 'why': '', 'game': 'ark', 'char': 'y', 'meta': {},
               'lines': {'skillup': {str(i): {'材': 99} for i in range(400)}}}
        r = EN.format_prog_receipt(big)
        self.assertIn('表太长已截断', r)
        self.assertLessEqual(len(r), EN.PROG_RECEIPT_BUDGET + 200)


class TestProgExchange(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix='gd-prog-x-')
        self.paths = EN.Paths(self.dir)
        self.paths.settings.parent.mkdir(parents=True, exist_ok=True)
        self.paths.settings.write_text('{}', encoding='utf-8')
        self.calls = []

    def tearDown(self):
        import shutil
        shutil.rmtree(self.dir, ignore_errors=True)

    def _prog(self, char, line, stage):
        self.calls.append((char, line, stage))
        return {'ok': True, 'why': '', 'game': 'r1999', 'char': char,
                'meta': {'updated': '2026-09-28'},
                'lines': {'insight': {'III': {'币': 200000}}}}

    @staticmethod
    def _acts(char):
        return '{"reply":"查表","actions":[{"type":"progression-lookup","char":"%s"}]}' % char

    def _run(self, queue, prog_fn):
        holder = {'i': 0}

        def model(msgs):
            txt = queue[holder['i']] if holder['i'] < len(queue) else '{"reply":"完","actions":[]}'
            holder['i'] += 1
            return {'Ok': True, 'Text': txt}

        history = []
        EN.send_exchange(self.paths, '37 洞悉三要多少', history, model,
                         lambda a, allowed=False: None, snapshot_fn=lambda: '',
                         printer=lambda t: None, prog_fn=prog_fn)
        return history

    def test_receipt_into_next_round(self):
        history = self._run([self._acts('37'), '{"reply":"按表作答","actions":[]}'], self._prog)
        joined = '\n'.join(h['text'] for h in history)
        self.assertIn('养成表实查', joined)
        self.assertIn('200000', joined)
        self.assertEqual(self.calls, [('37', '', '')])

    def test_prog_fn_exception_becomes_receipt_not_crash(self):
        def bad(char, line, stage):
            raise RuntimeError('表文件炸了')
        history = self._run([self._acts('37'), '{"reply":"收到","actions":[]}'], bad)
        joined = '\n'.join(h['text'] for h in history)
        self.assertIn('查表出了意外', joined)
        self.assertIn('表文件炸了', joined)

    def test_no_prog_fn_engine_says_not_connected(self):
        history = []
        EN.send_exchange(self.paths, '查', history,
                         lambda m: {'Ok': True, 'Text': self._acts('37')},
                         lambda a, allowed=False: None, snapshot_fn=lambda: '',
                         printer=lambda t: None)          # 旧装配(对拍/训练)不接 prog_fn：动作过闸、回执如实说没接
        joined = '\n'.join(h['text'] for h in history)
        self.assertIn('没接 prog_fn', joined)


# ---------- 第六动词 farm-lookup(刷本效率走结构化表；推荐在数据层现算，golden 不含) ----------

class TestFarmGate(unittest.TestCase):
    def test_basic_and_case_insensitive(self):
        n = EN.test_action({'type': 'FARM-LOOKUP', 'material': ' 龙门币 '})
        self.assertTrue(n['Ok'])
        self.assertEqual(n['Material'], '龙门币')            # trim 进规整值
        self.assertEqual(n['Mode'], '')

    def test_missing_material_rejected(self):
        self.assertFalse(EN.test_action({'type': 'farm-lookup'})['Ok'])
        self.assertFalse(EN.check_farm_lookup({'material': '   '})['Ok'])

    def test_mode_whitelist(self):
        self.assertTrue(EN.check_farm_lookup({'material': 'x', 'mode': 'cheapest'})['Ok'])
        self.assertTrue(EN.check_farm_lookup({'material': 'x', 'mode': 'ALL'})['Ok'])  # 大小写归一
        self.assertFalse(EN.check_farm_lookup({'material': 'x', 'mode': 'bestest'})['Ok'])

    def test_length_limit(self):
        self.assertFalse(EN.check_farm_lookup({'material': '字' * (EN.FARM_MATERIAL_MAX + 1)})['Ok'])

    def test_game_is_not_taken_from_model(self):
        n = EN.test_action({'type': 'farm-lookup', 'material': '碳', 'game': 'ark'})
        self.assertTrue(n['Ok'])
        self.assertNotIn('Game', n)                          # game 由引擎按会话注入


class TestFarmLookupData(unittest.TestCase):
    """数据层用临时沙盒表钉死推荐口径，不依赖本机 state/drops(那是本地件)。"""

    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix='gd-farm-')
        # 龙门币：只有 CE 材料本(理智 10/20/36)→ 推荐应取最高档 36
        # 轻锰矿：只有主线概率掉落关(理智 15/18)→ 推荐应取理智最低 15
        # 芯片组：材料本(36)+剿灭(60)混在→ 材料本优先，取 36
        tab = {"_meta": {"updated": "2026-05-05"}, "by_material": {
            "龙门币": [
                {"stage": "CE-1_a", "name": "CE-1", "cost": 10, "kind": "材料本", "slot": "common"},
                {"stage": "CE-3_b", "name": "CE-3", "cost": 20, "kind": "材料本", "slot": "common"},
                {"stage": "CE-6_c", "name": "CE-6", "cost": 36, "kind": "材料本", "slot": "common"}],
            "轻锰矿": [
                {"stage": "3-2_x", "name": "3-2", "cost": 15, "kind": "关卡", "slot": "common"},
                {"stage": "4-7_y", "name": "4-7", "cost": 18, "kind": "关卡", "slot": "common"}],
            "近卫芯片组": [
                {"stage": "PR-D-2_z", "name": "PR-D-2", "cost": 36, "kind": "材料本", "slot": "common"},
                {"stage": "炎国 市区", "name": "市区", "cost": 60, "kind": "剿灭", "slot": "first"}],
            "技巧概要·卷1": [
                {"stage": "CA-1_k", "name": "CA-1", "cost": 10, "kind": "材料本", "slot": "common"},
                {"stage": "CA-5_m", "name": "CA-5", "cost": 30, "kind": "材料本", "slot": "common"}],
            "碳": [{"stage": "SK-1_c", "name": "SK-1", "cost": None, "kind": "材料本", "slot": "common"}],
        }}
        dd = os.path.join(self.dir, 'state', 'drops')
        os.makedirs(dd)
        with open(os.path.join(dd, 'ark.json'), 'w', encoding='utf-8') as f:
            f.write(json.dumps(tab, ensure_ascii=False))

    def tearDown(self):
        import shutil
        shutil.rmtree(self.dir, ignore_errors=True)

    def test_material_ben_recommends_top_tier(self):
        r = DR.lookup(self.dir, 'ark', '龙门币')
        self.assertTrue(r['ok'])
        self.assertEqual(r['best']['stage'], 'CE-6_c')       # 材料本取理智最高档
        self.assertEqual(r['ranked'][0]['stage'], 'CE-6_c')  # 降序展示

    def test_no_ben_recommends_cheapest(self):
        r = DR.lookup(self.dir, 'ark', '轻锰矿')
        self.assertEqual(r['best']['stage'], '3-2_x')        # 无专用本→理智最低

    def test_ben_beats_exterminatus(self):
        r = DR.lookup(self.dir, 'ark', '近卫芯片组')
        self.assertEqual(r['best']['stage'], 'PR-D-2_z')     # 材料本优先于剿灭
        self.assertEqual(r['series'], 'PR')

    def test_cheapest_mode_forces_low(self):
        r = DR.lookup(self.dir, 'ark', '龙门币', 'cheapest')
        self.assertEqual(r['best']['stage'], 'CE-1_a')       # cheapest 强制最低

    def test_all_mode_lists_everyone_no_best(self):
        r = DR.lookup(self.dir, 'ark', '近卫芯片组', 'all')
        self.assertIsNone(r['best'])
        self.assertEqual(r['total_sources'], 2)

    def test_unique_substring_resolves(self):
        r = DR.lookup(self.dir, 'ark', '近卫芯片')            # 是"近卫芯片组"的…不，反过来
        # "近卫芯片" 不是任何键的子串命中多个? 近卫芯片组 含 近卫芯片 → 唯一命中
        self.assertTrue(r['ok'])
        self.assertEqual(r['material'], '近卫芯片组')

    def test_none_cost_stage_still_recommended_as_ben(self):
        r = DR.lookup(self.dir, 'ark', '碳')                  # 材料本但 cost=None
        self.assertTrue(r['ok'])
        self.assertEqual(r['best']['stage'], 'SK-1_c')

    def test_misses_are_honest(self):
        self.assertFalse(DR.lookup(self.dir, 'r1999', 'x')['ok'])           # 没这游戏的表
        self.assertFalse(DR.lookup(self.dir, 'ark', '不存在的材料')['ok'])
        r = DR.lookup(self.dir, 'ark', '')
        self.assertFalse(r['ok'])
        self.assertIn('没给材料名', r['why'])


class TestFarmReceipt(unittest.TestCase):
    def test_ok_receipt_has_best_and_note(self):
        r = EN.format_farm_receipt({'ok': True, 'why': '', 'game': 'ark', 'material': '龙门币',
                                    'meta': {'updated': '2026-09-28'}, 'series': 'CE',
                                    'note': '材料本取最高档', 'total_sources': 6,
                                    'best': {'stage': 'CE-6_c', 'name': 'CE-6', 'cost': 36, 'kind': '材料本'},
                                    'ranked': [{'stage': 'CE-6_c', 'name': 'CE-6', 'cost': 36, 'kind': '材料本'}]})
        self.assertIn('刷本实查(ark · 材料「龙门币」· 数据采集 2026-09-28)', r)
        self.assertIn('最划算：CE-6', r)
        self.assertIn('36理智', r)

    def test_miss_and_none(self):
        self.assertIn('刷本表没查到：表内无材料',
                      EN.format_farm_receipt({'ok': False, 'why': '表内无材料「x」'}))
        self.assertIn('没给原因', EN.format_farm_receipt(None))

    def test_unknown_cost_shows_question(self):
        r = EN.format_farm_receipt({'ok': True, 'why': '', 'game': 'ark', 'material': '碳',
                                    'meta': {}, 'note': 'x', 'total_sources': 1,
                                    'best': {'stage': 'SK-1_c', 'name': 'SK-1', 'cost': None, 'kind': '材料本'},
                                    'ranked': []})
        self.assertIn('理智?', r)

    def test_receipt_with_eff_shows_community_footer(self):
        r = EN.format_farm_receipt({'ok': True, 'why': '', 'game': 'ark', 'material': '龙门币',
                                    'meta': {'updated': '2026-09-28'}, 'series': 'CE', 'has_eff': True,
                                    'note': '按每理智期望掉落件数排序(Penguin-Stats 社区统计)', 'total_sources': 2,
                                    'best': {'stage': 'CE-6_c', 'name': 'CE-6', 'cost': 36, 'kind': '材料本',
                                             'qty': 10000.0, 'eff': 277.778},
                                    'ranked': [{'stage': 'CE-6_c', 'name': 'CE-6', 'cost': 36, 'kind': '材料本',
                                                'qty': 10000.0, 'eff': 277.778}]})
        self.assertIn('每局≈10000.0件·277.778件/理智', r)
        self.assertIn('Penguin-Stats 社区统计', r)
        self.assertNotIn('不含每场数量/掉率', r)


class TestFarmEffPriority(unittest.TestCase):
    """DROP2：表内有 qty/eff(Penguin 统计)时按 件/理智 降序推荐；无 eff 关排后；cheapest 不受影响。"""

    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix='gd-eff-')
        tab = {"_meta": {"updated": "2026-09-28"}, "by_material": {
            "龙门币": [
                {"stage": "CE-6_c", "name": "CE-6", "cost": 36, "kind": "材料本", "slot": "common",
                 "qty": 10000.0, "eff": 277.778},
                {"stage": "10-8_m", "name": "10-8", "cost": 21, "kind": "关卡", "slot": "common",
                 "qty": 3480.0, "eff": 165.714},
                {"stage": "剿灭X_y", "name": "剿灭X", "cost": 60, "kind": "剿灭", "slot": "first"}],
            # 效率最高档不是理智最低关：证明按 eff 而非旧"最低理智"口径
            "轻锰矿": [
                {"stage": "3-2_x", "name": "3-2", "cost": 15, "kind": "关卡", "slot": "common",
                 "qty": 0.3, "eff": 0.02},
                {"stage": "5-4_y", "name": "5-4", "cost": 21, "kind": "关卡", "slot": "common",
                 "qty": 1.05, "eff": 0.05}],
        }}
        dd = os.path.join(self.dir, 'state', 'drops')
        os.makedirs(dd)
        with open(os.path.join(dd, 'ark.json'), 'w', encoding='utf-8') as f:
            f.write(json.dumps(tab, ensure_ascii=False))

    def tearDown(self):
        import shutil
        shutil.rmtree(self.dir, ignore_errors=True)

    def test_eff_ordering_beats_tier_rule(self):
        r = DR.lookup(self.dir, 'ark', '轻锰矿')
        self.assertTrue(r['has_eff'])
        self.assertEqual(r['best']['stage'], '5-4_y')      # eff 高者优先，哪怕理智更高
        self.assertIn('Penguin-Stats', r['note'])

    def test_no_eff_sources_sort_last(self):
        r = DR.lookup(self.dir, 'ark', '龙门币')
        self.assertEqual(r['best']['stage'], 'CE-6_c')
        stages = [e['stage'] for e in r['ranked']]
        self.assertEqual(stages[-1], '剿灭X_y')            # 无统计关沉底

    def test_cheapest_mode_ignores_eff(self):
        # cheapest 恒按理智升序：CE-6(eff最高)让位给理智最低的 10-8(21)
        r = DR.lookup(self.dir, 'ark', '龙门币', 'cheapest')
        self.assertEqual(r['best']['stage'], '10-8_m')

    def test_wiki_fixed_source_named_in_note(self):
        # 固定产出线(龙门币这类 penguin 不统计的)：note 要说 wiki 出处，不冒充社区统计
        with io.open(os.path.join(self.dir, 'state', 'drops', 'ark.json'), encoding='utf-8') as f:
            tab = json.load(f)
        tab['by_material']['高级材料'] = [
            {"stage": "4-1_w", "name": "4-1", "cost": 18, "kind": "关卡", "slot": "common",
             "qty": 2700, "eff": 150.0, "src": "wiki-fixed"}]
        with io.open(os.path.join(self.dir, 'state', 'drops', 'ark.json'), 'w', encoding='utf-8') as f:
            f.write(json.dumps(tab, ensure_ascii=False))
        r = DR.lookup(self.dir, 'ark', '高级材料')
        self.assertTrue(r['has_eff'])
        self.assertIn('wiki 效率表固定产出', r['note'])
        self.assertNotIn('Penguin-Stats', r['note'])

    def test_best_alt_is_top_non_ben(self):
        # "CE 没开"现算替补：排除 CE 系列后效率最高的 10-8，无统计的剿灭关不顶位
        r = DR.lookup(self.dir, 'ark', '龙门币')
        self.assertEqual(r['best_alt']['stage'], '10-8_m')

    def test_receipt_shows_alt_line(self):
        r = EN.format_farm_receipt(DR.lookup(self.dir, 'ark', '龙门币', 'all'))
        self.assertIn('非材料本替补(该系列没开/打不了专用本时用)：10-8', r)

    def test_fallback_when_no_stats_at_all(self):
        # 只有无 eff 的材料：走旧材料本最高档回退口径，note 点明回退
        with io.open(os.path.join(self.dir, 'state', 'drops', 'ark.json'), encoding='utf-8') as f:
            tab = json.load(f)
        tab['by_material']['技巧之章'] = [
            {"stage": "CA-1_k", "name": "CA-1", "cost": 10, "kind": "材料本", "slot": "common"},
            {"stage": "CA-5_m", "name": "CA-5", "cost": 30, "kind": "材料本", "slot": "common"}]
        with open(os.path.join(self.dir, 'state', 'drops', 'ark.json'), 'w', encoding='utf-8') as f:
            f.write(json.dumps(tab, ensure_ascii=False))
        r = DR.lookup(self.dir, 'ark', '技巧之章')
        self.assertFalse(r['has_eff'])
        self.assertEqual(r['best']['stage'], 'CA-5_m')      # 旧口径：材料本最高档
        self.assertIn('回退档位口径', r['note'])


class TestFarmExchange(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix='gd-farm-x-')
        self.paths = EN.Paths(self.dir)
        self.paths.settings.parent.mkdir(parents=True, exist_ok=True)
        self.paths.settings.write_text('{}', encoding='utf-8')
        self.calls = []

    def tearDown(self):
        import shutil
        shutil.rmtree(self.dir, ignore_errors=True)

    def _farm(self, material, mode=''):
        self.calls.append((material, mode))
        return {'ok': True, 'why': '', 'game': 'ark', 'material': material,
                'meta': {'updated': '2026-09-28'}, 'note': 'n', 'series': 'CE', 'total_sources': 1,
                'best': {'stage': 'CE-6_c', 'name': 'CE-6', 'cost': 36, 'kind': '材料本'},
                'ranked': [{'stage': 'CE-6_c', 'name': 'CE-6', 'cost': 36, 'kind': '材料本'}]}

    @staticmethod
    def _acts(material):
        return '{"reply":"查刷本表","actions":[{"type":"farm-lookup","material":"%s"}]}' % material

    def _run(self, queue, farm_fn):
        holder = {'i': 0}

        def model(msgs):
            txt = queue[holder['i']] if holder['i'] < len(queue) else '{"reply":"完","actions":[]}'
            holder['i'] += 1
            return {'Ok': True, 'Text': txt}

        history = []
        EN.send_exchange(self.paths, '刷龙门币去哪划算', history, model,
                         lambda a, allowed=False: None, snapshot_fn=lambda: '',
                         printer=lambda t: None, farm_fn=farm_fn)
        return history

    def test_receipt_into_next_round(self):
        history = self._run([self._acts('龙门币'), '{"reply":"按表作答","actions":[]}'], self._farm)
        joined = '\n'.join(h['text'] for h in history)
        self.assertIn('刷本实查', joined)
        self.assertIn('最划算：CE-6', joined)
        self.assertEqual(self.calls, [('龙门币', '')])

    def test_farm_fn_exception_becomes_receipt_not_crash(self):
        def bad(material, mode=''):
            raise RuntimeError('掉落表炸了')
        history = self._run([self._acts('碳'), '{"reply":"收到","actions":[]}'], bad)
        joined = '\n'.join(h['text'] for h in history)
        self.assertIn('查表出了意外', joined)
        self.assertIn('掉落表炸了', joined)

    def test_no_farm_fn_engine_says_not_connected(self):
        history = []
        EN.send_exchange(self.paths, '查', history,
                         lambda m: {'Ok': True, 'Text': self._acts('碳')},
                         lambda a, allowed=False: None, snapshot_fn=lambda: '',
                         printer=lambda t: None)      # 旧装配不接 farm_fn：动作过闸、回执如实说没接
        joined = '\n'.join(h['text'] for h in history)
        self.assertIn('没接 farm_fn', joined)

    def test_empty_reply_not_printed(self):
        """教材 §6 话术纪律：纯查表轮 reply 为空是正常的，不许印出"agent："空行刷存在感。"""
        lines = []
        holder = {'i': 0}
        queue = ['{"reply":"","actions":[{"type":"farm-lookup","material":"龙门币"}]}',
                 '{"reply":"刷 10-8。","actions":[]}']

        def model(msgs):
            txt = queue[holder['i']] if holder['i'] < len(queue) else '{"reply":"完","actions":[]}'
            holder['i'] += 1
            return {'Ok': True, 'Text': txt}

        EN.send_exchange(self.paths, 'CE 没开刷龙门币去哪', [], model,
                         lambda a, allowed=False: None, snapshot_fn=lambda: '',
                         printer=lines.append, farm_fn=self._farm)
        agent_lines = [l for l in lines if l.startswith('agent：')]
        self.assertEqual(agent_lines, ['agent：刷 10-8。'])     # 只有结论那一句，没有空行
        self.assertTrue(any(l.startswith('(回执)') for l in lines))


class TestPickKbGames(unittest.TestCase):
    """P5 live 装配纯函数：kbAgentGames 列表优先，没配回落单值 kbAgentGame(旧行为逐字不变)。"""

    def test_games_list_wins(self):
        self.assertEqual(LV.pick_kb_games({'kbAgentGames': ['r1999', 'ark'], 'kbAgentGame': 'r1999'}),
                         ['r1999', 'ark'])

    def test_strip_dedupe_keep_order(self):
        self.assertEqual(LV.pick_kb_games({'kbAgentGames': [' ark ', 'ark', 'r1999']}), ['ark', 'r1999'])

    def test_fallback_single(self):
        self.assertEqual(LV.pick_kb_games({'kbAgentGame': 'ark'}), ['ark'])
        self.assertEqual(LV.pick_kb_games({'kbAgentGames': [], 'kbAgentGame': 'ark'}), ['ark'])
        self.assertEqual(LV.pick_kb_games({'kbAgentGames': [' ', ''], 'kbAgentGame': 'x'}), ['x'])

    def test_empty_means_no_domain(self):
        self.assertEqual(LV.pick_kb_games({}), [])

    def test_env_override_pure(self):
        """P5 补刀：GDAGENT_KB_GAME 设了就优先(域=[X])；未设/空白=None(照走 settings)。"""
        self.assertEqual(LV.kb_game_override({'GDAGENT_KB_GAME': 'ark'}), ['ark'])
        self.assertEqual(LV.kb_game_override({'GDAGENT_KB_GAME': ' ark '}), ['ark'])
        self.assertIsNone(LV.kb_game_override({}))
        self.assertIsNone(LV.kb_game_override({'GDAGENT_KB_GAME': '  '}))
        self.assertIsNone(LV.kb_game_override(None))


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
