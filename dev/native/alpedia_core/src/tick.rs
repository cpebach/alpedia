//! 1ティックの物理（world.step のサブステップの繰り返し）をまとめて行う。
//!
//! 動いている構築物（ブロックごと・剛体・完全剛体・関節つき剛体）それぞれのサブステップの前半、
//! MOD の箱（街の建物など。止まっている物だけ）との接触、構築物どうしの押し出し、固定された物の戻し、
//! 後半（速度）、結合した直後の速さのおさえ、を Python に戻らずにサブステップの数だけ繰り返す。
//! 眠っていた物が起きた・ブロックごとの計算でちぎれた、など Python の処理が要ることが起きたら、
//! そのサブステップで止めて Python に返す（残りのサブステップは Python が今までどおり進める）。

use crate::artstep::{self, ArtTopo};
use crate::body::{self, PlanetCtx, RContact, Topo};
use crate::contacts::{self, Proxy};
use crate::kernels;

/// MOD の箱のまとまり（止まっている持ち主1つ分）
pub struct BoxGroup {
    pub c: Vec<f64>,    // (n,3)
    pub a: Vec<f64>,    // (n,3,3)
    pub e: Vec<f64>,    // (n,3)
    pub part: Vec<i64>, // 箱ごとの部分の番号（建物の損傷用）
    pub lo: [f64; 3],   // 外枠（r を足す前）
    pub hi: [f64; 3],
}

impl BoxGroup {
    pub fn new(c: Vec<f64>, a: Vec<f64>, e: Vec<f64>, part: Vec<i64>) -> Self {
        let n = c.len() / 3;
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for i in 0..n {
            let rb = (e[3 * i].powi(2) + e[3 * i + 1].powi(2) + e[3 * i + 2].powi(2)).sqrt();
            for k in 0..3 {
                lo[k] = lo[k].min(c[3 * i + k] - rb);
                hi[k] = hi[k].max(c[3 * i + k] + rb);
            }
        }
        BoxGroup { c, a, e, part, lo, hi }
    }
}

pub struct SoftPart<'a> {
    pub rest: &'a mut [f64],
    pub act_scale: &'a [f64],
    pub last_r: &'a mut [f64],
    pub quat: &'a mut [f64],
    pub angles: Vec<f64>,
    pub parallel: bool,
}

pub struct RigidPart<'a> {
    pub full: bool,
    pub rb_r: &'a mut [f64], // (3,3)。後半ごとに更新
    pub rb_ib: &'a [f64],
    pub local: &'a [f64],
    pub rb_x: &'a mut [f64], // (3,)
}

pub struct ArtPart<'a> {
    pub at: &'a ArtTopo,
    pub x: &'a mut [f64],
    pub r: &'a mut [f64],
    pub vl: &'a mut [f64],
    pub vl0: &'a [f64],
    pub applied: &'a mut [f64],
    pub ext_applied: &'a mut [f64],
    pub want: Vec<f64>,
    pub rot_speed: Vec<f64>,
    pub ext_want: Vec<f64>,
    pub ext_speed: Vec<f64>,
}

pub enum Part<'a> {
    Soft(SoftPart<'a>),
    Rigid(RigidPart<'a>),
    Art(ArtPart<'a>),
}

pub struct Job<'a> {
    pub t: &'a Topo,
    pub proxy: &'a Proxy,
    pub pos: &'a mut [f64],
    pub vel: &'a mut [f64],
    pub prev: &'a mut [f64],
    pub force: &'a [f64],
    pub cav: Vec<[i64; 6]>,
    pub part: Part<'a>,
    pub boxes: Vec<usize>,
    pub contact_rigid: bool,
    pub settle_cap: f64, // 1サブステップの速さの変化の上限（0 なら無し）
}

pub struct Sleeper<'a> {
    pub proxy: &'a Proxy,
    pub pos: &'a mut [f64],
    pub vel: &'a [f64],
    pub rigid: bool,
    pub fixed: bool,    // アンカーで固定：押されても戻す
    pub can_wake: bool, // 押されたら起きる（拡張が位置を決めている物は起きない）
}

/// 建物の損傷の記録：(動いている構築物, 箱のまとまり, 部分, 速さ, サブステップ)
pub type Hit = (usize, usize, i64, f64, usize);

#[derive(Default)]
pub struct Outcome {
    pub done: usize,
    pub woke: Vec<usize>,
    pub touch: Vec<bool>,
    pub want_soft: Vec<bool>,
    pub broken: Vec<(usize, Vec<i64>, Vec<i64>)>,
    pub hits: Vec<Hit>,
}

/// MOD の箱との接触（construct._collide_boxes）。collect なら接触点を返し、そうでなければその場で押し出す
#[allow(clippy::too_many_arguments)]
fn box_contacts(
    job_k: usize, step: usize, g_k: usize, g: &BoxGroup, pos: &mut [f64], prev: &[f64], vel: &[f64], t: &Topo, collect: bool,
    hits: &mut Vec<Hit>,
) -> Vec<(usize, f64, [f64; 3])> {
    let r = t.size * 0.5;
    let nv = pos.len() / 3;
    let mut k = vec![0.0; pos.len()];
    kernels::contact_cores(pos, &t.ctype, &t.cidx, &mut k);
    let mut sel_p = Vec::new();
    let mut rows = Vec::new();
    for i in 0..nv {
        let q = [k[3 * i], k[3 * i + 1], k[3 * i + 2]];
        if (0..3).all(|a| q[a] > g.lo[a] - r && q[a] < g.hi[a] + r) {
            sel_p.extend_from_slice(&q);
            rows.push(i);
        }
    }
    let mut out = Vec::new();
    if rows.is_empty() {
        return out;
    }
    let m = rows.len();
    let (mut d, mut n, mut kk) = (vec![0.0; m], vec![0.0; 3 * m], vec![0i64; m]);
    kernels::box_sdf(&sel_p, &g.c, &g.a, &g.e, &mut d, &mut n, &mut kk);
    let mut best: Vec<(i64, f64)> = Vec::new();
    for j in 0..m {
        let dj = d[j] - r;
        if !(dj < 0.0) {
            continue;
        }
        let row = rows[j];
        let nj = [n[3 * j], n[3 * j + 1], n[3 * j + 2]];
        let vn = -(vel[3 * row] * nj[0] + vel[3 * row + 1] * nj[1] + vel[3 * row + 2] * nj[2]);
        if vn >= 7.0 {
            let pa = g.part[kk[j] as usize];
            match best.iter_mut().find(|b| b.0 == pa) {
                Some(b) => b.1 = b.1.max(vn),
                None => best.push((pa, vn)),
            }
        }
        out.push((row, -dj, nj));
    }
    for (pa, v) in best {
        hits.push((job_k, g_k, pa, v, step));
    }
    if !collect {
        for &(row, dep, nj) in &out {
            for a in 0..3 {
                pos[3 * row + a] += nj[a] * dep;
            }
            let disp = [pos[3 * row] - prev[3 * row], pos[3 * row + 1] - prev[3 * row + 1], pos[3 * row + 2] - prev[3 * row + 2]];
            let dn = disp[0] * nj[0] + disp[1] * nj[1] + disp[2] * nj[2];
            let fr = t.fric[row].clamp(0.0, 1.0);
            for a in 0..3 {
                pos[3 * row + a] -= (disp[a] - nj[a] * dn) * fr;
            }
        }
        out.clear();
    }
    out
}

/// 関節つき剛体の、前半で集めた接触（後半で使う）
#[derive(Default)]
struct ArtState {
    defo: Vec<(usize, [f64; 3])>,
    cg: Vec<i64>,
    cl: Vec<f64>,
    cn: Vec<f64>,
    lam: Vec<f64>,
}

fn limit_dv(vel: &mut [f64], v0: &[f64], cap: f64) {
    let nv = vel.len() / 3;
    let mut m: f64 = 0.0;
    for i in 0..nv {
        let d = [vel[3 * i] - v0[3 * i], vel[3 * i + 1] - v0[3 * i + 1], vel[3 * i + 2] - v0[3 * i + 2]];
        m = m.max(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
    }
    let m = m.sqrt();
    if m <= cap {
        return;
    }
    let k = cap / m;
    for i in 0..vel.len() {
        vel[i] = v0[i] + (vel[i] - v0[i]) * k;
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    jobs: &mut [Job], sleepers: &mut [Sleeper], groups: &[BoxGroup], planets: &[PlanetCtx], scale: f64, h: f64,
    substeps: usize, iters: i64, impact_speed: f64, soft: f64,
) -> Outcome {
    let nj = jobs.len();
    let ns = sleepers.len();
    let mut o = Outcome { touch: vec![false; nj + ns], want_soft: vec![false; nj + ns], ..Default::default() };
    let fixed0: Vec<Option<Vec<f64>>> = sleepers.iter().map(|s| if s.fixed { Some(s.pos.to_vec()) } else { None }).collect();
    // 眠っている物の当たり判定の点はティックの間変わらない（押されて動いたら起きるので、そこで止める）
    let spts: Vec<Vec<f64>> = sleepers.iter().map(|s| contacts::proxy_points_of(s.proxy, s.pos)).collect();
    let mut arts: Vec<ArtState> = (0..nj).map(|_| ArtState::default()).collect();
    let mut pt = std::time::Instant::now();
    let kind_slot = |p: &Part| match p { Part::Soft(_) => 13, Part::Rigid(_) => 14, Part::Art(_) => 15 };
    for step in 0..substeps {
        // ---- 前半 ----
        let mut v0s: Vec<Option<Vec<f64>>> = Vec::with_capacity(nj);
        for (k, j) in jobs.iter_mut().enumerate() {
            v0s.push(if j.settle_cap > 0.0 { Some(j.vel.to_vec()) } else { None });
            let slot = kind_slot(&j.part);
            let t = j.t;
            match &mut j.part {
                Part::Soft(s) => {
                    body::soft_begin(
                        j.pos, j.vel, j.prev, j.force, s.rest, s.act_scale, s.last_r, s.quat, t, planets, scale, h, iters,
                        &s.angles, s.parallel, &j.cav, soft,
                    );
                    for &g in &j.boxes {
                        box_contacts(k, step, g, &groups[g], j.pos, j.prev, j.vel, t, false, &mut o.hits);
                    }
                }
                Part::Rigid(rg) => {
                    let defer = rg.full && !j.boxes.is_empty();
                    let w = body::rigid_begin(
                        j.pos, j.vel, j.prev, j.force, rg.rb_r, rg.rb_ib, t, planets, scale, h, &j.cav, rg.full, impact_speed,
                        soft, defer,
                    );
                    o.want_soft[k] |= w;
                    if defer {
                        let mut bx: Vec<RContact> = Vec::new();
                        for &g in &j.boxes {
                            for (row, dep, nn) in box_contacts(k, step, g, &groups[g], j.pos, j.prev, j.vel, t, true, &mut o.hits) {
                                bx.push((row, dep, nn, [0.0; 3], g as i64));
                            }
                        }
                        let (w2, _) = body::full_contacts(j.pos, j.prev, j.vel, t, planets, h, impact_speed, soft, bx);
                        o.want_soft[k] |= w2;
                    } else if !rg.full {
                        for &g in &j.boxes {
                            box_contacts(k, step, g, &groups[g], j.pos, j.prev, j.vel, t, false, &mut o.hits);
                        }
                    }
                }
                Part::Art(a) => {
                    let (rows, d) = artstep::predict(
                        j.pos, j.vel, j.prev, j.force, t, a.at, a.x, a.r, a.vl, a.vl0, a.ext_applied, &a.ext_want, &a.ext_speed,
                        planets, scale, h, &j.cav,
                    );
                    let mut bx: Vec<artstep::Contact> = Vec::new();
                    for &g in &j.boxes {
                        for (row, dep, nn) in box_contacts(k, step, g, &groups[g], j.pos, j.prev, j.vel, t, true, &mut o.hits) {
                            bx.push(artstep::Contact { p: row, d: dep, n: nn, vs: [0.0; 3], ci: g as i64 });
                        }
                    }
                    let (kept, cl, lam) = artstep::solve(
                        j.pos, j.prev, t, a.at, a.x, a.r, a.vl, a.applied, &a.want, &a.rot_speed, planets, h, &bx,
                    );
                    let st = &mut arts[k];
                    st.defo = rows.into_iter().zip(d).collect();
                    st.cg = kept.iter().map(|c| a.at.vg[c.p]).collect();
                    st.cn = kept.iter().flat_map(|c| c.n).collect();
                    st.cl = cl;
                    st.lam = lam;
                }
            }
            crate::prof::add(slot, &mut pt);
        }
        // ---- 構築物どうしの押し出し ----
        let woke = {
            let mut items: Vec<contacts::Item> = Vec::with_capacity(nj + ns);
            for j in jobs.iter_mut() {
                items.push(contacts::Item { p: j.proxy, pos: &mut *j.pos, vel: &*j.vel, active: true, rigid: j.contact_rigid, pts: None });
            }
            for (s, sp) in sleepers.iter_mut().zip(spts.iter()) {
                items.push(contacts::Item { p: s.proxy, pos: &mut *s.pos, vel: s.vel, active: false, rigid: s.rigid, pts: Some(sp) });
            }
            let r = contacts::solve(&mut items, nj);
            for (q, (&t_, &w)) in r.touch.iter().zip(r.want_soft.iter()).enumerate() {
                o.touch[q] |= t_;
                o.want_soft[q] |= w;
            }
            r.woke
        };
        for (s, f) in sleepers.iter_mut().zip(fixed0.iter()) {
            if let Some(p0) = f {
                s.pos.copy_from_slice(p0);
            }
        }
        crate::prof::add(16, &mut pt);
        // ---- 後半 ----
        let mut stop = false;
        for (k, j) in jobs.iter_mut().enumerate() {
            let t = j.t;
            let slot = kind_slot(&j.part);
            match &mut j.part {
                Part::Soft(s) => {
                    let (br, rot) = body::soft_end(j.pos, j.vel, j.prev, s.rest, s.act_scale, s.last_r, t, h, &s.angles);
                    if !br.is_empty() || !rot.is_empty() {
                        o.broken.push((k, br, rot));
                        stop = true;
                    }
                }
                Part::Rigid(rg) => {
                    let (r, x) = body::rigid_end(j.pos, j.vel, j.prev, rg.local, &t.mass, h);
                    rg.rb_r.copy_from_slice(&r);
                    rg.rb_x.copy_from_slice(&x);
                }
                Part::Art(a) => {
                    let st = &arts[k];
                    artstep::end(j.pos, j.vel, j.prev, t, a.at, a.x, a.r, a.vl, &st.defo, &st.cg, &st.cl, &st.cn, &st.lam, h);
                }
            }
            if let Some(v0) = &v0s[k] {
                limit_dv(j.vel, v0, j.settle_cap);
            }
            crate::prof::add(slot, &mut pt);
        }
        o.done = step + 1;
        let woke: Vec<usize> = woke.into_iter().filter(|&q| q >= nj && sleepers[q - nj].can_wake).map(|q| q - nj).collect();
        if !woke.is_empty() {
            o.woke = woke;
            stop = true;
        }
        if stop {
            break;
        }
    }
    o
}
