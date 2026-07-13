//! DBA-path constants reproduced from `libatrac.so.1.2.0`.

#[rustfmt::skip]
pub const DBA_GAIN_TABLE: [f32; 24] = [
    f32::from_bits(0x3d800000), f32::from_bits(0x3e000000), f32::from_bits(0x3e800000), f32::from_bits(0x3f000000),
    f32::from_bits(0x3f800000), f32::from_bits(0x40000000), f32::from_bits(0x40800000), f32::from_bits(0x41000000),
    f32::from_bits(0x41800000), f32::from_bits(0x42000000), f32::from_bits(0x42800000), f32::from_bits(0x43000000),
    f32::from_bits(0x43800000), f32::from_bits(0x44000000), f32::from_bits(0x44800000), f32::from_bits(0x45000000),
    f32::from_bits(0x3d800000), f32::from_bits(0x3d7b95c2), f32::from_bits(0x3d7837f0), f32::from_bits(0x3d75fed7),
    f32::from_bits(0x3d7504f3), f32::from_bits(0x3d75672a), f32::from_bits(0x3d7744fd), f32::from_bits(0x3d7ac0c7),
];

#[rustfmt::skip]
pub const DBA_MDCT_PERM: [i32; 128] = [
    0, 127, 63, 64, 31, 96, 32, 95, 15, 112, 48, 79, 16, 111, 47, 80,
    7, 120, 56, 71, 24, 103, 39, 88, 8, 119, 55, 72, 23, 104, 40, 87,
    3, 124, 60, 67, 28, 99, 35, 92, 12, 115, 51, 76, 19, 108, 44, 83,
    4, 123, 59, 68, 27, 100, 36, 91, 11, 116, 52, 75, 20, 107, 43, 84,
    1, 126, 62, 65, 30, 97, 33, 94, 14, 113, 49, 78, 17, 110, 46, 81,
    6, 121, 57, 70, 25, 102, 38, 89, 9, 118, 54, 73, 22, 105, 41, 86,
    2, 125, 61, 66, 29, 98, 34, 93, 13, 114, 50, 77, 18, 109, 45, 82,
    5, 122, 58, 69, 26, 101, 37, 90, 10, 117, 53, 74, 21, 106, 42, 85,
];

#[rustfmt::skip]
pub const DBA_QMF_COEFFICIENTS: [f32; 23] = [
    f32::from_bits(0x40c98030), f32::from_bits(0xc1a4cf95), f32::from_bits(0x42696089), f32::from_bits(0xc30b226f),
    f32::from_bits(0x43902c9a), f32::from_bits(0xc4061394), f32::from_bits(0x4465dd4f), f32::from_bits(0xc4b9da9a),
    f32::from_bits(0x4511be69), f32::from_bits(0xc56841b1), f32::from_bits(0x45d47227), f32::from_bits(0xc6f817c8),
    f32::from_bits(0xc60d2ada), f32::from_bits(0x453a6288), f32::from_bits(0xc4a0d480), f32::from_bits(0x44056a7f),
    f32::from_bits(0xc32874f2), f32::from_bits(0x4085e56b), f32::from_bits(0x424ee4e5), f32::from_bits(0xc256555c),
    f32::from_bits(0x420e6eb3), f32::from_bits(0xc184914f), f32::from_bits(0x4075d95b),
];

#[rustfmt::skip]
pub const DBA_HCSPEC: [u32; 14] = [
    0x000c4100, 0x00000000, 0x000c40e0, 0x00000000, 0x000c40c0, 0x000c40a0, 0x000c4060, 0x00000000,
    0x000c4020, 0x000c3fe0, 0x000c3f60, 0x00000000, 0x000c3e60, 0x000c3d60,
];

#[rustfmt::skip]
pub const DBA_HCSPEC01: [u32; 16] = [
    0x00010000, 0x00038000, 0x00010000, 0x0003a000, 0x0004c000, 0x0005e000, 0x00010000, 0x0005e800,
    0x00010000, 0x00010000, 0x00010000, 0x00010000, 0x0004d000, 0x0005f000, 0x00010000, 0x0005f800,
];

#[rustfmt::skip]
pub const DBA_NORM_FACT: [f32; 21] = [
    f32::from_bits(0x41f1e7a4), f32::from_bits(0x421863f8), f32::from_bits(0x42400000), f32::from_bits(0x4249965e),
    f32::from_bits(0x427dfbf2), f32::from_bits(0x42a00000), f32::from_bits(0x428d1c75), f32::from_bits(0x42b1c9f7),
    f32::from_bits(0x42e00000), f32::from_bits(0x42b56dbb), f32::from_bits(0x42e495f4), f32::from_bits(0x43100000),
    f32::from_bits(0x431730c6), f32::from_bits(0x433e7cf6), f32::from_bits(0x43700000), f32::from_bits(0x439c3aef),
    f32::from_bits(0x43c4d676), f32::from_bits(0x43f80000), f32::from_bits(0x441ec003), f32::from_bits(0x44480335),
    f32::from_bits(0x447c0000),
];

#[rustfmt::skip]
pub const DBA_FCB: [f32; 127] = [
    f32::from_bits(0x3f000278), f32::from_bits(0x3f0009df), f32::from_bits(0x4222fa90), f32::from_bits(0x3f002785),
    f32::from_bits(0x3f32d693), f32::from_bits(0x41a2fdb4), f32::from_bits(0x3f374844), f32::from_bits(0x3f009e8d),
    f32::from_bits(0x3f09db19), f32::from_bits(0x3f30bc3a), f32::from_bits(0x3fac5c00), f32::from_bits(0x41230a46),
    f32::from_bits(0x3f0b43bb), f32::from_bits(0x3f39a180), f32::from_bits(0x3fa27099), f32::from_bits(0x3f0281f7),
    f32::from_bits(0x3f02331d), f32::from_bits(0x3f093163), f32::from_bits(0x402ed346), f32::from_bits(0x3f2cc03d),
    f32::from_bits(0x3f1b3a92), f32::from_bits(0x3fb1d462), f32::from_bits(0x3f624170), f32::from_bits(0x40a33c9c),
    f32::from_bits(0x3f02d63e), f32::from_bits(0x3f0c02fa), f32::from_bits(0x401a8199), f32::from_bits(0x3f3e99ee),
    f32::from_bits(0x3f18b425), f32::from_bits(0x3f9dee3b), f32::from_bits(0x3f6ab8f0), f32::from_bits(0x3f0a8bd4),
    f32::from_bits(0x3f007947), f32::from_bits(0x3f01e9a1), f32::from_bits(0x40ba7c6c), f32::from_bits(0x3f07f268),
    f32::from_bits(0x3f2748a0), f32::from_bits(0x403b2d1d), f32::from_bits(0x3f46cf4d), f32::from_bits(0x3f25961d),
    f32::from_bits(0x3f064503), f32::from_bits(0x3f1c8f06), f32::from_bits(0x3fd3ea96), f32::from_bits(0x3fbdf91b),
    f32::from_bits(0x3f1033e2), f32::from_bits(0x3f5e4bd7), f32::from_bits(0x3f8af7ba), f32::from_bits(0x402406cf),
    f32::from_bits(0x3f00c8e1), f32::from_bits(0x3f033005), f32::from_bits(0x4091294c), f32::from_bits(0x3f0d9838),
    f32::from_bits(0x3f23f296), f32::from_bits(0x40120d1c), f32::from_bits(0x3f4cd897), f32::from_bits(0x3f49c480),
    f32::from_bits(0x3f054608), f32::from_bits(0x3f17816b), f32::from_bits(0x3fe5c949), f32::from_bits(0x3f95b035),
    f32::from_bits(0x3f121b66), f32::from_bits(0x3f6f40df), f32::from_bits(0x3f84babf), f32::from_bits(0x3f3504f3),
    f32::from_bits(0x3f001638), f32::from_bits(0x3f005907), f32::from_bits(0x41595941), f32::from_bits(0x3f01668b),
    f32::from_bits(0x3f2eb50d), f32::from_bits(0x40d97efb), f32::from_bits(0x3f3c11af), f32::from_bits(0x3f05c278),
    f32::from_bits(0x3f088e89), f32::from_bits(0x3f290ab8), f32::from_bits(0x3fb7afe3), f32::from_bits(0x405a1642),
    f32::from_bits(0x3f0cc9bf), f32::from_bits(0x3f43f76d), f32::from_bits(0x3f99affd), f32::from_bits(0x3f19f1bd),
    f32::from_bits(0x3f01a576), f32::from_bits(0x3f06cdc5), f32::from_bits(0x40497005), f32::from_bits(0x3f1f5c6e),
    f32::from_bits(0x3f1def80), f32::from_bits(0x3fcc0749), f32::from_bits(0x3f5a8162), f32::from_bits(0x3fdc7926),
    f32::from_bits(0x3f038f5d), f32::from_bits(0x3f0f4d13), f32::from_bits(0x400a7e49), f32::from_bits(0x3f56df9e),
    f32::from_bits(0x3f165939), f32::from_bits(0x3f8e585d), f32::from_bits(0x3f74000f), f32::from_bits(0x3fa73d75),
    f32::from_bits(0x3f003dc8), f32::from_bits(0x3f00f84c), f32::from_bits(0x4102762a), f32::from_bits(0x3f03f45b),
    f32::from_bits(0x3f2add09), f32::from_bits(0x4082b522), f32::from_bits(0x3f413b6c), f32::from_bits(0x3f11233f),
    f32::from_bits(0x3f075cdd), f32::from_bits(0x3f225d7c), f32::from_bits(0x3fc4bc39), f32::from_bits(0x4003b2af),
    f32::from_bits(0x3f0e6e97), f32::from_bits(0x3f500d3f), f32::from_bits(0x3f91e9d7), f32::from_bits(0x3f6664d7),
    f32::from_bits(0x3f012cd7), f32::from_bits(0x3f04cf99), f32::from_bits(0x406dc689), f32::from_bits(0x3f153b3a),
    f32::from_bits(0x3f20d644), f32::from_bits(0x3feff562), f32::from_bits(0x3f536446), f32::from_bits(0x3f87c449),
    f32::from_bits(0x3f045f12), f32::from_bits(0x3f131c9a), f32::from_bits(0x3ffb1cd9), f32::from_bits(0x3f78fa3b),
    f32::from_bits(0x3f14271e), f32::from_bits(0x3f81d821), f32::from_bits(0x3f7e337a),
];

#[rustfmt::skip]
pub const DBA_FRTBL: [f32; 512] = [
    f32::from_bits(0x3f81949b), f32::from_bits(0xaf14a8bc), f32::from_bits(0xaae98391), f32::from_bits(0x3f00009e),
    f32::from_bits(0x3f84cced), f32::from_bits(0xae43c472), f32::from_bits(0xaae6a3b7), f32::from_bits(0x3f00058d),
    f32::from_bits(0x3f8819ec), f32::from_bits(0xade7fd97), f32::from_bits(0xaae3c62b), f32::from_bits(0x3f000f6d),
    f32::from_bits(0x3f8b7c40), f32::from_bits(0xada39e05), f32::from_bits(0xaae0eaf6), f32::from_bits(0x3f001e40),
    f32::from_bits(0x3f8ef499), f32::from_bits(0xad7b44d1), f32::from_bits(0xaade121d), f32::from_bits(0x3f003207),
    f32::from_bits(0x3f9283ab), f32::from_bits(0xad4aee5b), f32::from_bits(0xaadb3baa), f32::from_bits(0x3f004ac8),
    f32::from_bits(0x3f962a34), f32::from_bits(0xad29785f), f32::from_bits(0xaad867a3), f32::from_bits(0x3f006886),
    f32::from_bits(0x3f99e8fa), f32::from_bits(0xad10ef88), f32::from_bits(0xaad59610), f32::from_bits(0x3f008b48),
    f32::from_bits(0x3f9dc0ca), f32::from_bits(0xacfc5aba), f32::from_bits(0xaad2c6fa), f32::from_bits(0x3f00b315),
    f32::from_bits(0x3fa1b27a), f32::from_bits(0xacdebded), f32::from_bits(0xaacffa67), f32::from_bits(0x3f00dff3),
    f32::from_bits(0x3fa5beeb), f32::from_bits(0xacc6c696), f32::from_bits(0xaacd3061), f32::from_bits(0x3f0111ed),
    f32::from_bits(0x3fa9e706), f32::from_bits(0xacb2fbb6), f32::from_bits(0xaaca68ef), f32::from_bits(0x3f01490b),
    f32::from_bits(0x3fae2bbd), f32::from_bits(0xaca25cee), f32::from_bits(0xaac7a41a), f32::from_bits(0x3f018559),
    f32::from_bits(0x3fb28e10), f32::from_bits(0xac9435d9), f32::from_bits(0xaac4e1eb), f32::from_bits(0x3f01c6e2),
    f32::from_bits(0x3fb70f07), f32::from_bits(0xac8803d2), f32::from_bits(0xaac22268), f32::from_bits(0x3f020db4),
    f32::from_bits(0x3fbbafb8), f32::from_bits(0xac7acbdb), f32::from_bits(0xaabf659d), f32::from_bits(0x3f0259dd),
    f32::from_bits(0x3fc07145), f32::from_bits(0xac68257b), f32::from_bits(0xaabcab90), f32::from_bits(0x3f02ab6c),
    f32::from_bits(0x3fc554dd), f32::from_bits(0xac57a33b), f32::from_bits(0xaab9f44b), f32::from_bits(0x3f030271),
    f32::from_bits(0x3fca5bbd), f32::from_bits(0xac48ec50), f32::from_bits(0xaab73fd7), f32::from_bits(0x3f035efe),
    f32::from_bits(0x3fcf8730), f32::from_bits(0xac3bba26), f32::from_bits(0xaab48e3d), f32::from_bits(0x3f03c126),
    f32::from_bits(0x3fd4d892), f32::from_bits(0xac2fd3ed), f32::from_bits(0xaab1df86), f32::from_bits(0x3f0428fe),
    f32::from_bits(0x3fda514e), f32::from_bits(0xac250b69), f32::from_bits(0xaaaf33bc), f32::from_bits(0x3f04969a),
    f32::from_bits(0x3fdff2e1), f32::from_bits(0xac1b3a94), f32::from_bits(0xaaac8ae8), f32::from_bits(0x3f050a12),
    f32::from_bits(0x3fe5bedb), f32::from_bits(0xac1241e3), f32::from_bits(0xaaa9e513), f32::from_bits(0x3f05837f),
    f32::from_bits(0x3febb6de), f32::from_bits(0xac0a06ef), f32::from_bits(0xaaa74247), f32::from_bits(0x3f0602f8),
    f32::from_bits(0x3ff1dca1), f32::from_bits(0xac027378), f32::from_bits(0xaaa4a28e), f32::from_bits(0x3f06889b),
    f32::from_bits(0x3ff831f2), f32::from_bits(0xabf6e92f), f32::from_bits(0xaaa205f2), f32::from_bits(0x3f071484),
    f32::from_bits(0x3ffeb8b4), f32::from_bits(0xabe9f44f), f32::from_bits(0xaa9f6c7d), f32::from_bits(0x3f07a6d2),
    f32::from_bits(0x4002b973), f32::from_bits(0xabddec8d), f32::from_bits(0xaa9cd639), f32::from_bits(0x3f083fa4),
    f32::from_bits(0x4006314d), f32::from_bits(0xabd2b9e9), f32::from_bits(0xaa9a4330), f32::from_bits(0x3f08df1d),
    f32::from_bits(0x4009c503), f32::from_bits(0xabc84788), f32::from_bits(0xaa97b36c), f32::from_bits(0x3f098560),
    f32::from_bits(0x400d75bd), f32::from_bits(0xabbe8336), f32::from_bits(0xaa9526f9), f32::from_bits(0x3f0a3294),
    f32::from_bits(0x401144b0), f32::from_bits(0xabb55cfd), f32::from_bits(0xaa929de0), f32::from_bits(0x3f0ae6df),
    f32::from_bits(0x40153326), f32::from_bits(0xabacc6cd), f32::from_bits(0xaa90182d), f32::from_bits(0x3f0ba26d),
    f32::from_bits(0x40194278), f32::from_bits(0xaba4b439), f32::from_bits(0xaa8d95ea), f32::from_bits(0x3f0c6569),
    f32::from_bits(0x401d7411), f32::from_bits(0xab9d1a3a), f32::from_bits(0xaa8b1722), f32::from_bits(0x3f0d3001),
    f32::from_bits(0x4021c974), f32::from_bits(0xab95eefe), f32::from_bits(0xaa889be1), f32::from_bits(0x3f0e0267),
    f32::from_bits(0x40264434), f32::from_bits(0xab8f29bf), f32::from_bits(0xaa862432), f32::from_bits(0x3f0edcce),
    f32::from_bits(0x402ae5fe), f32::from_bits(0xab88c2a0), f32::from_bits(0xaa83b020), f32::from_bits(0x3f0fbf6d),
    f32::from_bits(0x402fb098), f32::from_bits(0xab82b292), f32::from_bits(0xaa813fb7), f32::from_bits(0x3f10aa7b),
    f32::from_bits(0x4034a5e1), f32::from_bits(0xab79e66a), f32::from_bits(0xaa7da604), f32::from_bits(0x3f119e35),
    f32::from_bits(0x4039c7d2), f32::from_bits(0xab6efd93), f32::from_bits(0xaa78d41b), f32::from_bits(0x3f129adb),
    f32::from_bits(0x403f1884), f32::from_bits(0xab64a031), f32::from_bits(0xaa7409cb), f32::from_bits(0x3f13a0ae),
    f32::from_bits(0x40449a31), f32::from_bits(0xab5ac4cc), f32::from_bits(0xaa6f472b), f32::from_bits(0x3f14aff4),
    f32::from_bits(0x404a4f32), f32::from_bits(0xab5162c7), f32::from_bits(0xaa6a8c55), f32::from_bits(0x3f15c8f8),
    f32::from_bits(0x40503a08), f32::from_bits(0xab487246), f32::from_bits(0xaa65d961), f32::from_bits(0x3f16ec07),
    f32::from_bits(0x40565d59), f32::from_bits(0xab3fec19), f32::from_bits(0xaa612e69), f32::from_bits(0x3f181972),
    f32::from_bits(0x405cbbf6), f32::from_bits(0xab37c9ad), f32::from_bits(0xaa5c8b85), f32::from_bits(0x3f19518f),
    f32::from_bits(0x406358df), f32::from_bits(0xab3004f8), f32::from_bits(0xaa57f0d0), f32::from_bits(0x3f1a94ba),
    f32::from_bits(0x406a3744), f32::from_bits(0xab28986e), f32::from_bits(0xaa535e65), f32::from_bits(0x3f1be352),
    f32::from_bits(0x40715a8b), f32::from_bits(0xab217ef3), f32::from_bits(0xaa4ed45c), f32::from_bits(0x3f1d3dbb),
    f32::from_bits(0x4078c651), f32::from_bits(0xab1ab3d0), f32::from_bits(0xaa4a52d2), f32::from_bits(0x3f1ea461),
    f32::from_bits(0x40803f3a), f32::from_bits(0xab1432aa), f32::from_bits(0xaa45d9e1), f32::from_bits(0x3f2017b5),
    f32::from_bits(0x40844388), f32::from_bits(0xab0df779), f32::from_bits(0xaa4169a5), f32::from_bits(0x3f21982c),
    f32::from_bits(0x40887248), f32::from_bits(0xab07fe83), f32::from_bits(0xaa3d0239), f32::from_bits(0x3f232644),
    f32::from_bits(0x408ccdd4), f32::from_bits(0xab02444f), f32::from_bits(0xaa38a3bb), f32::from_bits(0x3f24c283),
    f32::from_bits(0x409158b1), f32::from_bits(0xaaf98b4c), f32::from_bits(0xaa344e46), f32::from_bits(0x3f266d75),
    f32::from_bits(0x40961593), f32::from_bits(0xaaeeff11), f32::from_bits(0xaa3001f8), f32::from_bits(0x3f2827af),
    f32::from_bits(0x409b075f), f32::from_bits(0xaae4de57), f32::from_bits(0xaa2bbeed), f32::from_bits(0x3f29f1cf),
    f32::from_bits(0x40a03131), f32::from_bits(0xaadb23e9), f32::from_bits(0xaa278544), f32::from_bits(0x3f2bcc7b),
    f32::from_bits(0x40a59660), f32::from_bits(0xaad1cae9), f32::from_bits(0xaa23551b), f32::from_bits(0x3f2db866),
    f32::from_bits(0x40ab3a84), f32::from_bits(0xaac8ceca), f32::from_bits(0xaa1f2e91), f32::from_bits(0x3f2fb64b),
    f32::from_bits(0x40b1217e), f32::from_bits(0xaac02b4b), f32::from_bits(0xaa1b11c4), f32::from_bits(0x3f31c6f4),
    f32::from_bits(0x40b74f78), f32::from_bits(0xaab7dc6d), f32::from_bits(0xaa16fed5), f32::from_bits(0x3f33eb34),
    f32::from_bits(0x40bdc8f3), f32::from_bits(0xaaafde73), f32::from_bits(0xaa12f5e2), f32::from_bits(0x3f3623ee),
    f32::from_bits(0x40c492cf), f32::from_bits(0xaaa82dd6), f32::from_bits(0xaa0ef70c), f32::from_bits(0x3f387214),
    f32::from_bits(0x40cbb250), f32::from_bits(0xaaa0c74b), f32::from_bits(0xaa0b0275), f32::from_bits(0x3f3ad6a7),
    f32::from_bits(0x40d32d29), f32::from_bits(0xaa99a7b3), f32::from_bits(0xaa07183d), f32::from_bits(0x3f3d52ba),
    f32::from_bits(0x40db098b), f32::from_bits(0xaa92cc21), f32::from_bits(0xaa033886), f32::from_bits(0x3f3fe771),
    f32::from_bits(0x40e34e30), f32::from_bits(0xaa8c31d3), f32::from_bits(0xa9fec6e7), f32::from_bits(0x3f429607),
    f32::from_bits(0x40ec0267), f32::from_bits(0xaa85d62c), f32::from_bits(0xa9f7324e), f32::from_bits(0x3f455fca),
    f32::from_bits(0x40f52e26), f32::from_bits(0xaa7f6d70), f32::from_bits(0xa9efb389), f32::from_bits(0x3f484624),
    f32::from_bits(0x40feda20), f32::from_bits(0xaa73a246), f32::from_bits(0xa9e84ae0), f32::from_bits(0x3f4b4a95),
    f32::from_bits(0x410487e8), f32::from_bits(0xaa684673), f32::from_bits(0xa9e0f89a), f32::from_bits(0x3f4e6ebd),
    f32::from_bits(0x4109ecce), f32::from_bits(0xaa5d55cd), f32::from_bits(0xa9d9bd04), f32::from_bits(0x3f51b458),
    f32::from_bits(0x410fa174), f32::from_bits(0xaa52cc63), f32::from_bits(0xa9d29867), f32::from_bits(0x3f551d47),
    f32::from_bits(0x4115ac18), f32::from_bits(0xaa48a678), f32::from_bits(0xa9cb8b11), f32::from_bits(0x3f58ab90),
    f32::from_bits(0x411c139d), f32::from_bits(0xaa3ee07e), f32::from_bits(0xa9c4954e), f32::from_bits(0x3f5c6160),
    f32::from_bits(0x4122df97), f32::from_bits(0xaa357718), f32::from_bits(0xa9bdb76e), f32::from_bits(0x3f604116),
    f32::from_bits(0x412a1862), f32::from_bits(0xaa2c6711), f32::from_bits(0xa9b6f1c2), f32::from_bits(0x3f644d3d),
    f32::from_bits(0x4131c742), f32::from_bits(0xaa23ad60), f32::from_bits(0xa9b0449b), f32::from_bits(0x3f68889d),
    f32::from_bits(0x4139f67a), f32::from_bits(0xaa1b471e), f32::from_bits(0xa9a9b04b), f32::from_bits(0x3f6cf638),
    f32::from_bits(0x4142b171), f32::from_bits(0xaa13318a), f32::from_bits(0xa9a33528), f32::from_bits(0x3f719955),
    f32::from_bits(0x414c04da), f32::from_bits(0xaa0b6a05), f32::from_bits(0xa99cd387), f32::from_bits(0x3f767586),
    f32::from_bits(0x4155fee3), f32::from_bits(0xaa03ee0d), f32::from_bits(0xa9968bbf), f32::from_bits(0x3f7b8eb3),
    f32::from_bits(0x4160af69), f32::from_bits(0xa9f97681), f32::from_bits(0xa9905e2a), f32::from_bits(0x3f807491),
    f32::from_bits(0x416c2837), f32::from_bits(0xa9eb9ead), f32::from_bits(0xa98a4b22), f32::from_bits(0x3f8344bf),
    f32::from_bits(0x41787d52), f32::from_bits(0xa9de5048), f32::from_bits(0xa9845303), f32::from_bits(0x3f863a79),
    f32::from_bits(0x4182e2a9), f32::from_bits(0xa9d18727), f32::from_bits(0xa97cec55), f32::from_bits(0x3f895892),
    f32::from_bits(0x418a0ce2), f32::from_bits(0xa9c53f54), f32::from_bits(0xa97169f2), f32::from_bits(0x3f8ca22d),
    f32::from_bits(0x4191cbd7), f32::from_bits(0xa9b97500), f32::from_bits(0xa9661f9f), f32::from_bits(0x3f901ac1),
    f32::from_bits(0x419a300b), f32::from_bits(0xa9ae2488), f32::from_bits(0xa95b0e22), f32::from_bits(0x3f93c624),
    f32::from_bits(0x41a34c5f), f32::from_bits(0xa9a34a73), f32::from_bits(0xa9503645), f32::from_bits(0x3f97a89f),
    f32::from_bits(0x41ad367c), f32::from_bits(0xa998e369), f32::from_bits(0xa94598d6), f32::from_bits(0x3f9bc6f7),
    f32::from_bits(0x41b80750), f32::from_bits(0xa98eec38), f32::from_bits(0xa93b36a8), f32::from_bits(0x3fa02685),
    f32::from_bits(0x41c3dbad), f32::from_bits(0xa98561cf), f32::from_bits(0xa9311090), f32::from_bits(0x3fa4cd49),
    f32::from_bits(0x41d0d50f), f32::from_bits(0xa978827c), f32::from_bits(0xa927276a), f32::from_bits(0x3fa9c208),
    f32::from_bits(0x41df1a81), f32::from_bits(0xa9670f63), f32::from_bits(0xa91d7c13), f32::from_bits(0x3faf0c69),
    f32::from_bits(0x41eed9cc), f32::from_bits(0xa95664e7), f32::from_bits(0xa9140f71), f32::from_bits(0x3fb4b51e),
    f32::from_bits(0x42002471), f32::from_bits(0xa9467dd3), f32::from_bits(0xa90ae26a), f32::from_bits(0x3fbac60f),
    f32::from_bits(0x4209d3d8), f32::from_bits(0xa9375524), f32::from_bits(0xa901f5ec), f32::from_bits(0x3fc14a99),
    f32::from_bits(0x4214a13a), f32::from_bits(0xa928e609), f32::from_bits(0xa8f295d2), f32::from_bits(0x3fc84fcd),
    f32::from_bits(0x4220ba4e), f32::from_bits(0xa91b2be1), f32::from_bits(0xa8e1c4b1), f32::from_bits(0x3fcfe4c9),
    f32::from_bits(0x422e5655), f32::from_bits(0xa90e2236), f32::from_bits(0xa8d17a6d), f32::from_bits(0x3fd81b26),
    f32::from_bits(0x423db88d), f32::from_bits(0xa901c4be), f32::from_bits(0xa8c1b90b), f32::from_bits(0x3fe1077a),
    f32::from_bits(0x424f3379), f32::from_bits(0xa8ec1ea8), f32::from_bits(0xa8b2829b), f32::from_bits(0x3feac202),
    f32::from_bits(0x42632d3f), f32::from_bits(0xa8d5fbfa), f32::from_bits(0xa8a3d938), f32::from_bits(0x3ff56781),
    f32::from_bits(0x427a2582), f32::from_bits(0xa8c119c1), f32::from_bits(0xa895bf09), f32::from_bits(0x40008d2b),
    f32::from_bits(0x428a5ebb), f32::from_bits(0xa8ad7096), f32::from_bits(0xa8883644), f32::from_bits(0x400701f5),
    f32::from_bits(0x4299e182), f32::from_bits(0xa89af94f), f32::from_bits(0xa8768252), f32::from_bits(0x400e2b51),
    f32::from_bits(0x42ac2048), f32::from_bits(0xa889ad07), f32::from_bits(0xa85dc40f), f32::from_bits(0x4016282f),
    f32::from_bits(0x42c1c9cb), f32::from_bits(0xa8730a25), f32::from_bits(0xa8463678), f32::from_bits(0x401f1f03),
    f32::from_bits(0x42dbc6ed), f32::from_bits(0xa854f603), f32::from_bits(0xa82fde65), f32::from_bits(0x4029402d),
    f32::from_bits(0x42fb538d), f32::from_bits(0xa8391135), f32::from_bits(0xa81ac0c9), f32::from_bits(0x4034c962),
    f32::from_bits(0x4311125d), f32::from_bits(0xa81f4fb4), f32::from_bits(0xa806e2ba), f32::from_bits(0x40420aa4),
    f32::from_bits(0x4329528d), f32::from_bits(0xa807a5dd), f32::from_bits(0xa7e892d4), f32::from_bits(0x40516d8e),
    f32::from_bits(0x43482bc8), f32::from_bits(0xa7e410de), f32::from_bits(0xa7c5f45e), f32::from_bits(0x40638075),
    f32::from_bits(0x43703f53), f32::from_bits(0xa7bcd90b), f32::from_bits(0xa7a5f502), f32::from_bits(0x407907e9),
    f32::from_bits(0x4392d155), f32::from_bits(0xa7998f2a), f32::from_bits(0xa7889ff6), f32::from_bits(0x40898d9f),
    f32::from_bits(0x43b77a65), f32::from_bits(0xa7743d9e), f32::from_bits(0xa75c017b), f32::from_bits(0x4099aa9b),
    f32::from_bits(0x43ebc320), f32::from_bits(0xa73ce861), f32::from_bits(0xa72c464c), f32::from_bits(0x40ae15d7),
    f32::from_bits(0x441cff5c), f32::from_bits(0xa70cf845), f32::from_bits(0xa702269c), f32::from_bits(0x40c8cc0d),
    f32::from_bits(0x445b57f6), f32::from_bits(0xa6c89036), f32::from_bits(0xa6bb76a1), f32::from_bits(0x40ed3bf0),
    f32::from_bits(0x44a3df32), f32::from_bits(0xa68567af), f32::from_bits(0xa67c77df), f32::from_bits(0x4110f0a5),
    f32::from_bits(0x450778ea), f32::from_bits(0xa6206201), f32::from_bits(0xa619a351), f32::from_bits(0x413a5065),
    f32::from_bits(0x4584c86a), f32::from_bits(0xa5a2a130), f32::from_bits(0xa59db719), f32::from_bits(0x41826673),
    f32::from_bits(0x4638706d), f32::from_bits(0xa4e8baf6), f32::from_bits(0xa4e47c38), f32::from_bits(0x41d94fd4),
    f32::from_bits(0x47cf8126), f32::from_bits(0xa34d9873), f32::from_bits(0xa34c567d), f32::from_bits(0x42a2f9c6),
];

#[rustfmt::skip]
pub const DBA_NSPS: [i32; 32] = [
    8, 8, 8, 8, 8, 8, 8, 8, 16, 16, 16, 16,
    16, 16, 16, 16, 32, 32, 32, 32, 32, 32, 32, 32,
    32, 32, 64, 64, 64, 64, 128, 128,
];

#[rustfmt::skip]
pub const DBA_QTSTART: [i32; 33] = [
    0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112,
    128, 144, 160, 176, 192, 224, 256, 288, 320, 352, 384, 416,
    448, 480, 512, 576, 640, 704, 768, 896, 1024,
];

#[rustfmt::skip]
pub const DBA_DAT_000D3CC0: [i32; 8] = [
    1024, 1, 2, 2, 2, 4, 6, 6,
];

#[rustfmt::skip]
pub const DBA_ZEROBITS: [i32; 7] = [
    1, 2, 2, 2, 4, 6, 6,
];

#[rustfmt::skip]
pub const DBA_ATH_THRESHOLD: [i32; 32] = [
    7, 5, 5, 4, 4, 4, 4, 3, 3, 3, 3, 4,
    5, 5, 5, 6, 6, 7, 7, 8, 10, 13, 17, 22,
    28, 35, 49, 74, 109, 155, 250, 441,
];

#[rustfmt::skip]
pub const DBA_TONE_THRESH: [i32; 32] = [
    11, 9, 9, 9, 9, 9, 10, 13, 9, 8, 8, 8,
    8, 9, 10, 13, 8, 7, 7, 7, 8, 9, 10, 13,
    7, 7, 7, 7, 8, 9, 9, 9,
];

#[rustfmt::skip]
pub const DBA_BITCOUNT_R: [i32; 5] = [
    -225, -266, -307, -317, -1024,
];

#[rustfmt::skip]
pub const DBA_BITS0: [i32; 7] = [
    40, 40, 60, 76, 60, 60, 100,
];

#[rustfmt::skip]
pub const DBA_BITS1: [i32; 7] = [
    15, 20, 25, 29, 35, 45, 55,
];

#[rustfmt::skip]
pub const DBA_BITCOUNT_NEXT: [i32; 7] = [
    3, 5, 7, 9, 12, 15, 18,
];

pub fn dba_bitcount_r_view(presence: i32) -> i32 {
    const COMBINED: [i32; 8] = [
        DBA_BITCOUNT_R[4],
        DBA_BITS0[0],
        DBA_BITS0[1],
        DBA_BITS0[2],
        DBA_BITS0[3],
        DBA_BITS0[4],
        DBA_BITS0[5],
        DBA_BITS0[6],
    ];
    COMBINED[presence as usize]
}

pub fn dba_bitcount_bits0_view(presence: i32) -> i32 {
    const COMBINED: [i32; 8] = [
        DBA_BITS0[6],
        DBA_BITS1[0],
        DBA_BITS1[1],
        DBA_BITS1[2],
        DBA_BITS1[3],
        DBA_BITS1[4],
        DBA_BITS1[5],
        DBA_BITS1[6],
    ];
    COMBINED[presence as usize]
}

pub fn dba_bitcount_bits1_view(presence: i32) -> i32 {
    const COMBINED: [i32; 8] = [
        DBA_BITS1[6],
        DBA_BITCOUNT_NEXT[0],
        DBA_BITCOUNT_NEXT[1],
        DBA_BITCOUNT_NEXT[2],
        DBA_BITCOUNT_NEXT[3],
        DBA_BITCOUNT_NEXT[4],
        DBA_BITCOUNT_NEXT[5],
        DBA_BITCOUNT_NEXT[6],
    ];
    COMBINED[presence as usize]
}

#[rustfmt::skip]
pub const DBA_CHSWCOEF: [f32; 5] = [
    f32::from_bits(0x00000000), f32::from_bits(0x40000000), f32::from_bits(0x40000000), f32::from_bits(0x3f800000),
    f32::from_bits(0x3f800000),
];

#[rustfmt::skip]
pub const DBA_WT_COMP: [f32; 24] = [
    f32::from_bits(0x3fb504f3), f32::from_bits(0x3fc6610e), f32::from_bits(0x3fd5dba6), f32::from_bits(0x3fe35d3e),
    f32::from_bits(0x3feeba1f), f32::from_bits(0x3ff7a96b), f32::from_bits(0x3ffdb236), f32::from_bits(0x40000000),
    f32::from_bits(0x00000000), f32::from_bits(0x3e924925), f32::from_bits(0x3f124925), f32::from_bits(0x3f5b6db7),
    f32::from_bits(0x3f924925), f32::from_bits(0x3fb6db6e), f32::from_bits(0x3fdb6db7), f32::from_bits(0x40000000),
    f32::from_bits(0x403504f3), f32::from_bits(0x403417e9), f32::from_bits(0x4031495d), f32::from_bits(0x402c81d0),
    f32::from_bits(0x4025958d), f32::from_bits(0x401c3bb4), f32::from_bits(0x400ffb5b), f32::from_bits(0x40000000),
];

#[rustfmt::skip]
pub const DBA_SCALE_FACTOR_TABLE: [f32; 64] = [
    f32::from_bits(0x3d000000), f32::from_bits(0x3d214518), f32::from_bits(0x3d4b2ff5), f32::from_bits(0x3d800000),
    f32::from_bits(0x3da14518), f32::from_bits(0x3dcb2ff5), f32::from_bits(0x3e000000), f32::from_bits(0x3e214518),
    f32::from_bits(0x3e4b2ff5), f32::from_bits(0x3e800000), f32::from_bits(0x3ea14518), f32::from_bits(0x3ecb2ff5),
    f32::from_bits(0x3f000000), f32::from_bits(0x3f214518), f32::from_bits(0x3f4b2ff5), f32::from_bits(0x3f800000),
    f32::from_bits(0x3fa14518), f32::from_bits(0x3fcb2ff5), f32::from_bits(0x40000000), f32::from_bits(0x40214518),
    f32::from_bits(0x404b2ff5), f32::from_bits(0x40800000), f32::from_bits(0x40a14518), f32::from_bits(0x40cb2ff5),
    f32::from_bits(0x41000000), f32::from_bits(0x41214518), f32::from_bits(0x414b2ff5), f32::from_bits(0x41800000),
    f32::from_bits(0x41a14518), f32::from_bits(0x41cb2ff5), f32::from_bits(0x42000000), f32::from_bits(0x42214518),
    f32::from_bits(0x424b2ff5), f32::from_bits(0x42800000), f32::from_bits(0x42a14518), f32::from_bits(0x42cb2ff5),
    f32::from_bits(0x43000000), f32::from_bits(0x43214518), f32::from_bits(0x434b2ff5), f32::from_bits(0x43800000),
    f32::from_bits(0x43a14518), f32::from_bits(0x43cb2ff5), f32::from_bits(0x44000000), f32::from_bits(0x44214518),
    f32::from_bits(0x444b2ff5), f32::from_bits(0x44800000), f32::from_bits(0x44a14518), f32::from_bits(0x44cb2ff5),
    f32::from_bits(0x45000000), f32::from_bits(0x45214518), f32::from_bits(0x454b2ff5), f32::from_bits(0x45800000),
    f32::from_bits(0x45a14518), f32::from_bits(0x45cb2ff5), f32::from_bits(0x46000000), f32::from_bits(0x46214518),
    f32::from_bits(0x464b2ff5), f32::from_bits(0x46800000), f32::from_bits(0x46a14518), f32::from_bits(0x46cb2ff5),
    f32::from_bits(0x47000000), f32::from_bits(0x47214518), f32::from_bits(0x474b2ff5), f32::from_bits(0x47800000),
];

#[rustfmt::skip]
pub const DBA_NONTONE_HUFF_TABLE_PTRS: [u32; 16] = [
    0x0000002a, 0x00000055, 0x000c4100, 0x00000000, 0x000c40e0, 0x00000000, 0x000c40c0, 0x000c40a0,
    0x000c4060, 0x00000000, 0x000c4020, 0x000c3fe0, 0x000c3f60, 0x00000000, 0x000c3e60, 0x000c3d60,
];

#[rustfmt::skip]
pub const DBA_QTEND: [i32; 32] = [
    8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128,
    144, 160, 176, 192, 224, 256, 288, 320, 352, 384, 416, 448,
    480, 512, 576, 640, 704, 768, 896, 1024,
];

#[rustfmt::skip]
pub const DBA_NBITS_WL2_QUAD: [i32; 16] = [
    2, 4, 5, 6, 4, 6, 7, 8, 5, 7, 8, 9, 6, 8, 9, 10,
];

#[rustfmt::skip]
pub const DBA_HUF_MASK: [u32; 6] = [
    0x00000007, 0x00000007, 0x0000000f, 0x0000000f, 0x0000001f, 0x0000003f,
];

#[rustfmt::skip]
pub const DBA_HCSPEC02: [u32; 8] = [
    0x00010000, 0x00038000, 0x0003c000, 0x00010000, 0x00010000, 0x00010000, 0x0003e000, 0x0003a000,
];

#[rustfmt::skip]
pub const DBA_HCSPEC03: [u32; 8] = [
    0x00010000, 0x00038000, 0x0004c000, 0x0004e000, 0x00010000, 0x0004f000, 0x0004d000, 0x0003a000,
];

#[rustfmt::skip]
pub const DBA_HCSPEC04: [u32; 16] = [
    0x00010000, 0x00038000, 0x0004c000, 0x0005e000, 0x0005f000, 0x00010000, 0x00010000, 0x00010000,
    0x00010000, 0x00010000, 0x00010000, 0x00010000, 0x0005f800, 0x0005e800, 0x0004d000, 0x0003a000,
];

#[rustfmt::skip]
pub const DBA_HCSPEC05: [u32; 16] = [
    0x00020000, 0x00034000, 0x00048000, 0x0004a000, 0x0005e000, 0x0006f000, 0x0006f800, 0x0004c000,
    0x00020000, 0x0004d000, 0x0006fc00, 0x0006f400, 0x0005e800, 0x0004b000, 0x00049000, 0x00036000,
];

#[rustfmt::skip]
pub const DBA_HCSPEC06: [u32; 32] = [
    0x00030000, 0x00042000, 0x00044000, 0x00046000, 0x0005a000, 0x0005b000, 0x0005c000, 0x0006d000,
    0x0006d800, 0x0006e000, 0x0006e800, 0x0007f000, 0x0007f400, 0x0007f800, 0x0007fc00, 0x00048000,
    0x00030000, 0x00049000, 0x0007fe00, 0x0007fa00, 0x0007f600, 0x0007f200, 0x0006ec00, 0x0006e400,
    0x0006dc00, 0x0006d400, 0x0005c800, 0x0005b800, 0x0005a800, 0x00047000, 0x00045000, 0x00043000,
];

#[rustfmt::skip]
pub const DBA_HCSPEC07: [u32; 64] = [
    0x00030000, 0x00054000, 0x00055000, 0x00056000, 0x00057000, 0x00058000, 0x00069000, 0x00069800,
    0x0006a000, 0x0006a800, 0x0006b000, 0x0006b800, 0x0006c000, 0x0006c800, 0x0007d000, 0x0007d400,
    0x0007d800, 0x0007dc00, 0x0007e000, 0x0007e400, 0x0007e800, 0x0008ec00, 0x0008ee00, 0x0008f000,
    0x0008f200, 0x0008f400, 0x0008f600, 0x0008f800, 0x0008fa00, 0x0008fc00, 0x0008fe00, 0x00042000,
    0x00030000, 0x00043000, 0x0008ff00, 0x0008fd00, 0x0008fb00, 0x0008f900, 0x0008f700, 0x0008f500,
    0x0008f300, 0x0008f100, 0x0008ef00, 0x0008ed00, 0x0007ea00, 0x0007e600, 0x0007e200, 0x0007de00,
    0x0007da00, 0x0007d600, 0x0007d200, 0x0006cc00, 0x0006c400, 0x0006bc00, 0x0006b400, 0x0006ac00,
    0x0006a400, 0x00069c00, 0x00069400, 0x00058800, 0x00057800, 0x00056800, 0x00055800, 0x00054800,
];

#[rustfmt::skip]
pub const DBA_HCSPEC13: [u32; 8] = [
    0x00030000, 0x00032000, 0x00034000, 0x00036000, 0x00030000, 0x0003a000, 0x0003c000, 0x0003e000,
];

#[rustfmt::skip]
pub const DBA_HCSPEC15: [u32; 16] = [
    0x00040000, 0x00041000, 0x00042000, 0x00043000, 0x00044000, 0x00045000, 0x00046000, 0x00047000,
    0x00040000, 0x00049000, 0x0004a000, 0x0004b000, 0x0004c000, 0x0004d000, 0x0004e000, 0x0004f000,
];

#[rustfmt::skip]
pub const DBA_HCSPEC17: [u32; 64] = [
    0x00060000, 0x00060400, 0x00060800, 0x00060c00, 0x00061000, 0x00061400, 0x00061800, 0x00061c00,
    0x00062000, 0x00062400, 0x00062800, 0x00062c00, 0x00063000, 0x00063400, 0x00063800, 0x00063c00,
    0x00064000, 0x00064400, 0x00064800, 0x00064c00, 0x00065000, 0x00065400, 0x00065800, 0x00065c00,
    0x00066000, 0x00066400, 0x00066800, 0x00066c00, 0x00067000, 0x00067400, 0x00067800, 0x00067c00,
    0x00060000, 0x00068400, 0x00068800, 0x00068c00, 0x00069000, 0x00069400, 0x00069800, 0x00069c00,
    0x0006a000, 0x0006a400, 0x0006a800, 0x0006ac00, 0x0006b000, 0x0006b400, 0x0006b800, 0x0006bc00,
    0x0006c000, 0x0006c400, 0x0006c800, 0x0006cc00, 0x0006d000, 0x0006d400, 0x0006d800, 0x0006dc00,
    0x0006e000, 0x0006e400, 0x0006e800, 0x0006ec00, 0x0006f000, 0x0006f400, 0x0006f800, 0x0006fc00,
];

pub fn dba_hcspec_table(idsf_idx: i32) -> &'static [u32] {
    match idsf_idx {
        2 => &DBA_HCSPEC02,
        3 => &DBA_HCSPEC03,
        4 => &DBA_HCSPEC04,
        5 => &DBA_HCSPEC05,
        6 => &DBA_HCSPEC06,
        7 => &DBA_HCSPEC07,
        _ => &[],
    }
}

pub fn dba_hcspec_packed_table(idsf_idx: i32, coding_layout: i32) -> &'static [u32] {
    match (idsf_idx, coding_layout) {
        (1, _) => &DBA_HCSPEC01,
        (2, 0) => &DBA_HCSPEC02,
        (3, 0) => &DBA_HCSPEC03,
        (3, 1) => &DBA_HCSPEC13,
        (4, 0) => &DBA_HCSPEC04,
        (5, 0) => &DBA_HCSPEC05,
        (5, 1) => &DBA_HCSPEC15,
        (6, 0) => &DBA_HCSPEC06,
        (7, 0) => &DBA_HCSPEC07,
        (7, 1) => &DBA_HCSPEC17,
        _ => &[],
    }
}

pub const DBA_SCALE_LOOKUP: [u32; 22] = {
    let mut arr = [0u32; 22];
    arr[0] = 0x0005f800;
    arr[1] = 0x41f1e7a4;
    arr[2] = 0x421863f8;
    arr[3] = 0x42400000;
    arr[4] = 0x4249965e;
    arr[5] = 0x427dfbf2;
    arr[6] = 0x42a00000;
    arr[7] = 0x428d1c75;
    arr[8] = 0x42b1c9f7;
    arr[9] = 0x42e00000;
    arr[10] = 0x42b56dbb;
    arr[11] = 0x42e495f4;
    arr[12] = 0x43100000;
    arr[13] = 0x431730c6;
    arr[14] = 0x433e7cf6;
    arr[15] = 0x43700000;
    arr[16] = 0x439c3aef;
    arr[17] = 0x43c4d676;
    arr[18] = 0x43f80000;
    arr[19] = 0x441ec003;
    arr[20] = 0x44480335;
    arr[21] = 0x447c0000;
    arr
};
