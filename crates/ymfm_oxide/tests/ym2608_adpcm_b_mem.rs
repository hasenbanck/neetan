mod common;

use common::harness::AdpcmTester;

fn normalize(s: &str) -> String {
    s.lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn adpcm_b_mem_rw() {
    let mut rs = AdpcmTester::new();

    rs.seq_mem_limit(0xFFFF);
    rs.seq_mem_write(0x0000, 0x0000, 0x00, 32, "WRITE ADDRESS 0000-0000 (00-1F)");
    rs.seq_mem_write(0x0FFF, 0x1000, 0x40, 64, "WRITE ADDRESS 0fff-1000 (40-7F)");
    rs.seq_mem_write(
        0x0FFF,
        0x0FFF,
        0x80,
        64,
        "WRITE ADDRESS 0fff-0xfff x 2 (80-9F/A0-BF)",
    );
    rs.seq_mem_write(0x1FFF, 0x1FFF, 0xC0, 32, "WRITE ADDRESS 1fff-1fff (C0-DF)");

    rs.seq_mem_read(0x0000, 0x0000, 34, "READ ADDRESS 0000-0000");
    rs.seq_mem_read(0x0FFF, 0x1000, 66, "READ ADDRESS 0fff-1000");
    rs.seq_mem_read(0x0FFF, 0x0FFF, 68, "READ ADDRESS 0fff-0xfff x 2");

    // test_mem_read_start(rs, 0x0fff, 0x1000, 0x1000, 1, 66+34-1)
    {
        rs.msg("READ ADDRESS 0fff- CHANGE START(1)");
        rs.out(0x10, 0x00).out(0x10, 0x80);
        rs.out(0x00, 0x20).out(0x01, 0x02);
        rs.out(0x02, 0xFF).out(0x03, 0x0F);
        rs.out(0x04, 0x00).out(0x05, 0x10);
        rs.nl();
        rs.mrd(1).nl();
        rs.out(0x02, 0x00).out(0x03, 0x10);
        rs.mrd(66 + 34 - 1).nl();
        rs.out(0x00, 0x00).out(0x10, 0x80).nl();
    }

    // test_mem_read_start(rs, 0x0fff, 0x1000, 0x1000, 10, 66+34-10)
    {
        rs.msg("READ ADDRESS 0fff- CHANGE START(10)");
        rs.out(0x10, 0x00).out(0x10, 0x80);
        rs.out(0x00, 0x20).out(0x01, 0x02);
        rs.out(0x02, 0xFF).out(0x03, 0x0F);
        rs.out(0x04, 0x00).out(0x05, 0x10);
        rs.nl();
        rs.mrd(10).nl();
        rs.out(0x02, 0x00).out(0x03, 0x10);
        rs.mrd(66 + 34 - 10).nl();
        rs.out(0x00, 0x00).out(0x10, 0x80).nl();
    }

    // test_mem_read_start(rs, 0x0fff, 0x0000, 0x1000, 10, 66+34-10)
    {
        rs.msg("READ ADDRESS 0fff- CHANGE START(10/RESET)");
        rs.out(0x10, 0x00).out(0x10, 0x80);
        rs.out(0x00, 0x20).out(0x01, 0x02);
        rs.out(0x02, 0xFF).out(0x03, 0x0F);
        rs.out(0x04, 0x00).out(0x05, 0x10);
        rs.nl();
        rs.mrd(10).nl();
        rs.out(0x02, 0x00).out(0x03, 0x00);
        rs.mrd(66 + 34 - 10).nl();
        rs.out(0x00, 0x00).out(0x10, 0x80).nl();
    }

    rs.reset();

    // test_mem_read_stop(rs, 0x0fff, 0x0fff, 0x1000, 10, 66+66-10)
    {
        rs.msg("READ ADDRESS 0fff- CHANGE STOP");
        rs.out(0x10, 0x00).out(0x10, 0x80);
        rs.out(0x00, 0x20).out(0x01, 0x02);
        rs.out(0x02, 0xFF).out(0x03, 0x0F);
        rs.out(0x04, 0xFF).out(0x05, 0x0F);
        rs.nl();
        rs.mrd(10).nl();
        rs.out(0x04, 0x00).out(0x05, 0x10);
        rs.mrd(66 + 66 - 10).nl();
        rs.out(0x00, 0x00).out(0x10, 0x80).nl();
    }

    rs.seq_mem_read(0x1FFF, 0x1FFF, 34, "READ ADDRESS 1fff-1fff");

    rs.msg("READ / WRITE MIX");
    rs.reset();

    // test_mem_read_write(rs, 0x0000, 0x0000, 10, 68-10-1)
    {
        rs.msg("READ ADDRESS 0000-0000 (10 WRITE)");
        rs.out(0x10, 0x00).out(0x10, 0x80);
        rs.out(0x00, 0x20).out(0x01, 0x02);
        rs.out(0x02, 0x00).out(0x03, 0x00);
        rs.out(0x04, 0x00).out(0x05, 0x00);
        rs.nl();
        rs.mrd(10).nl();
        rs.out(0x08, 0xCC).stat();
        rs.mrd(68 - 10 - 1).nl();
        rs.out(0x00, 0x00).out(0x10, 0x80).nl();
    }

    let expected = include_str!("golden/mem_rw.txt");
    assert_eq!(normalize(rs.output()), normalize(expected));
}

#[test]
fn adpcm_b_mem_rw13() {
    let mut rs = AdpcmTester::new();

    rs.reset();

    rs.seq_mem_limit(0xFFFF);
    rs.seq_mem_write(
        0x0000,
        0x0001,
        0x00,
        64,
        "1. WRITE ADDRESS 0000-0001 (00-3F)",
    );
    rs.seq_mem_write(
        0x0FFF,
        0x0FFF,
        0xA0,
        32,
        "2. WRITE ADDRESS 0fff-0fff (A0-BF)",
    );

    let mut testno = 3;
    let ivals: &[u16] = &[1, 2, 3, 4, 31, 32, 33, 34, 35, 36, 37];
    for &i in ivals {
        rs.reset();
        let label = format!("{}. READ ADDRESS 0000-0000 ({}B)", testno, i);
        // test_seq_mem_read with control1 = 0x20
        {
            rs.msg(&label);
            rs.out(0x10, 0x00).out(0x10, 0x80);
            rs.out(0x00, 0x20).out(0x01, 0x02);
            rs.out(0x02, 0x00).out(0x03, 0x00);
            rs.out(0x04, 0x00).out(0x05, 0x00);
            rs.nl();
            rs.mrd(i).nl();
            rs.out(0x00, 0x00).out(0x10, 0x80).nl();
        }
        testno += 1;
        rs.reset();
        let label = format!("{}. READ ADDRESS 0fff-0fff (DUMMY READ TEST)", testno);
        rs.seq_mem_read(0x0FFF, 0x0FFF, 32 + 2, &label);
        testno += 1;
    }

    let expected = include_str!("golden/mem_rw13.txt");
    assert_eq!(normalize(rs.output()), normalize(expected));
}

#[test]
fn adpcm_b_mem_rw14() {
    let mut rs = AdpcmTester::new();

    rs.reset();

    rs.seq_mem_limit(0xFFFF);
    rs.seq_mem_write(
        0x0000,
        0x0001,
        0x00,
        64,
        "1. WRITE ADDRESS 0000-0001 (00-3F)",
    );
    rs.seq_mem_write(
        0x0FFF,
        0x0FFF,
        0xA0,
        32,
        "2. WRITE ADDRESS 0fff-0fff (A0-BF)",
    );

    let mut testno = 3;
    let ivals: &[u16] = &[1, 2, 3, 4, 31, 32, 33, 34, 35, 36, 37];
    for &i in ivals {
        rs.reset();
        let label = format!("{}. READ ADDRESS 0000-0000 ({}B) (REPEAT=1)", testno, i);
        // test_seq_mem_read with control1 = 0x30
        {
            rs.msg(&label);
            rs.out(0x10, 0x00).out(0x10, 0x80);
            rs.out(0x00, 0x30).out(0x01, 0x02);
            rs.out(0x02, 0x00).out(0x03, 0x00);
            rs.out(0x04, 0x00).out(0x05, 0x00);
            rs.nl();
            rs.mrd(i).nl();
            rs.out(0x00, 0x00).out(0x10, 0x80).nl();
        }
        testno += 1;
        rs.reset();
        let label = format!("{}. READ ADDRESS 0fff-0fff (DUMMY READ TEST)", testno);
        rs.seq_mem_read(0x0FFF, 0x0FFF, 32 + 2, &label);
        testno += 1;
    }

    let expected = include_str!("golden/mem_rw14.txt");
    assert_eq!(normalize(rs.output()), normalize(expected));
}

#[test]
fn adpcm_b_mem_rw15() {
    let mut rs = AdpcmTester::new();

    rs.seq_mem_limit(0xFFFF);
    rs.reset();

    // test_mem_write with control1 = 0x70
    fn test_mem_write(
        rs: &mut AdpcmTester,
        start: u16,
        stop: u16,
        data: u8,
        count: u16,
        message: &str,
    ) {
        rs.msg(message);
        rs.out(0x10, 0x00).out(0x10, 0x80);
        rs.out(0x00, 0x70).out(0x01, 0x02);
        rs.out(0x02, (start & 0xFF) as u8)
            .out(0x03, ((start >> 8) & 0xFF) as u8);
        rs.out(0x04, (stop & 0xFF) as u8)
            .out(0x05, ((stop >> 8) & 0xFF) as u8);
        rs.nl();
        rs.mwr(data, count).nl();
        rs.out(0x00, 0x00).out(0x10, 0x80).nl();
    }

    test_mem_write(
        &mut rs,
        0x0000,
        0x0000,
        0x00,
        32,
        "1. WRITE ADDRESS 0000-0000 (00-1F)",
    );

    let mut testno = 2;
    for i in 0..4u16 {
        rs.reset();
        let label = format!(
            "{}. WRITE ADDRESS 0fff-0fff (40-5F...) ({}B)",
            testno,
            32 + i
        );
        test_mem_write(&mut rs, 0x0FFF, 0x0FFF, 0x40, 32 + i, &label);
        testno += 1;
        rs.reset();
        let label = format!("{}. READ ADDRESS 0000-0000", testno);
        rs.seq_mem_read(0x0000, 0x0000, 34, &label);
        testno += 1;
    }

    let label = format!("{}. READ ADDRESS 0fff-0fff", testno);
    rs.reset();
    rs.seq_mem_read(0x0FFF, 0x0FFF, 34, &label);

    let expected = include_str!("golden/mem_rw15.txt");
    assert_eq!(normalize(rs.output()), normalize(expected));
}
