const std = @import("std");
const time = std.time;
const compile = @import("./compile.zig");

const c = @cImport({
    @cInclude("signal.h");
});

const assert = std.debug.assert;

const ArrayList = std.ArrayList;
const Diagnostic = @import("./Diagnostic.zig");
const Parser = @import("./parser/Parser.zig");

fn signalHandler(signal: c_int) callconv(.C) noreturn {
    _ = signal;
    std.debug.print("bye!\n", .{});
    std.process.exit(0);
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    _ = c.signal(c.SIGINT, signalHandler);

    var zlap = try @import("zlap").Zlap(@embedFile("./commands.zlap")).init(allocator);
    defer zlap.deinit();

    if (zlap.is_help or !zlap.isSubcmdActive("compile")) {
        std.debug.print("{s}\n", .{zlap.help_msg});
        return;
    }

    const compile_subcmd = zlap.subcommands.get("compile").?;
    const main_filenames = compile_subcmd.args.get("FILENAMES").?;
    const compile_lim: usize = blk: {
        const tmp = compile_subcmd.flags.get("lim").?.value.number;
        if (tmp <= 0) return error.InvalidCompileLimit;
        break :blk @intCast(tmp);
    };

    const watch = compile_subcmd.flags.get("watch").?.value.bool;

    const is_latex = compile_subcmd.flags.get("latex").?.value.bool;
    const is_pdflatex = compile_subcmd.flags.get("pdflatex").?.value.bool;
    const is_xelatex = compile_subcmd.flags.get("xelatex").?.value.bool;
    const is_lualatex = compile_subcmd.flags.get("lualatex").?.value.bool;

    const engine = try getEngine(is_latex, is_pdflatex, is_xelatex, is_lualatex);

    var diagnostic = Diagnostic{};
    defer diagnostic.deinit();

    var prev_mtime: ?i128 = null;
    try compile.compile(
        allocator,
        main_filenames.value.strings.items,
        &diagnostic,
        engine,
        compile_lim,
        &prev_mtime,
        watch,
    );
}

fn getEngine(
    is_latex: bool,
    is_pdflatex: bool,
    is_xelatex: bool,
    is_lualatex: bool,
) ![]const u8 {
    const is_latex_num = @as(u8, @intCast(@intFromBool(is_latex))) << 0;
    const is_pdflatex_num = @as(u8, @intCast(@intFromBool(is_pdflatex))) << 1;
    const is_xelatex_num = @as(u8, @intCast(@intFromBool(is_xelatex))) << 2;
    const is_lualatex_num = @as(u8, @intCast(@intFromBool(is_lualatex))) << 3;
    const engine_num = is_latex_num | is_pdflatex_num | is_xelatex_num | is_lualatex_num;

    switch (engine_num) {
        // TODO: read config file and replace with it
        0 => return "lualatex",
        1 << 0 => return "latex",
        1 << 1 => return "pdflatex",
        1 << 2 => return "xelatex",
        1 << 3 => return "lualatex",
        else => return error.InvalidEngineFlag,
    }
}

test "vesti tests" {
    _ = @import("./parser/Parser.zig");
}
