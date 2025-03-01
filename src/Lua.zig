const std = @import("std");
const ziglua = @import("ziglua");
const diag = @import("./diagnostic.zig");

const Allocator = std.mem.Allocator;
const ArrayList = std.ArrayList;
const ZigLua = ziglua.Lua;
const CowStr = @import("./CowStr.zig").CowStr;
const Parser = @import("./parser/Parser.zig");
const Codegen = @import("./Codegen.zig");

lua: *ZigLua,

const Self = @This();
pub const Error = Allocator.Error || ziglua.Error;

const VESTI_OUTPUT_STR: [:0]const u8 = "__VESTI_OUTPUT_STR__";
const VESTI_ERROR_STR: [:0]const u8 = "__VESTI_ERROR_STR__";

const VESTI_LUA_FUNCTIONS_BUILTINS: [4]struct {
    name: []const u8,
    val: fn (lua: *ZigLua) i32,
} = .{
    .{ .name = "sprint", .val = sprint },
    .{ .name = "sprintn", .val = sprintn },
    .{ .name = "sprintln", .val = sprintln },
    .{ .name = "parse", .val = parse },
};

pub fn init(allocator: Allocator) Error!Self {
    const lua = try ZigLua.init(allocator);
    errdefer lua.deinit();

    // open all standard libraries of lua
    lua.openLibs();

    // add global variables that vesti uses
    _ = lua.pushString("");
    lua.setGlobal(VESTI_OUTPUT_STR);
    _ = lua.pushNil();
    lua.setGlobal(VESTI_ERROR_STR);

    // declare vesti table
    lua.pushGlobalTable();
    lua.setGlobal("vesti");

    // this function throws error only if there is no global variable `vesti`.
    // however we already DEFINE the table
    _ = lua.getGlobal("vesti") catch unreachable;
    inline for (VESTI_LUA_FUNCTIONS_BUILTINS) |info| {
        _ = lua.pushString(info.name);
        lua.pushFunction(ziglua.wrap(info.val));
        lua.setTable(-3);
    }

    return Self{ .lua = lua };
}

pub fn deinit(self: Self) void {
    self.lua.deinit();
}

pub fn getError(self: Self) ?[:0]const u8 {
    const lua_ty = self.lua.getGlobal(VESTI_ERROR_STR) catch return null;
    return if (lua_ty == .string) self.lua.toString(-1) catch unreachable else null;
}

pub fn clearVestiOutputStr(self: Self) void {
    _ = self.lua.pushString("");
    self.lua.setGlobal(VESTI_OUTPUT_STR);
}

pub fn evalCode(self: Self, code: [:0]const u8) !void {
    try self.lua.doString(code);
}

pub fn getVestiOutputStr(self: Self) [:0]const u8 {
    _ = self.lua.getGlobal(VESTI_OUTPUT_STR) catch unreachable;
    return self.lua.toString(-1) catch unreachable;
}

fn sprint(lua: *ZigLua) i32 {
    if (lua.getTop() == 0) return 0;

    _ = lua.getGlobal(VESTI_OUTPUT_STR) catch unreachable;
    lua.rotate(1, 1);
    lua.concat(lua.getTop());
    lua.setGlobal(VESTI_OUTPUT_STR);

    return 0;
}

fn sprintn(lua: *ZigLua) i32 {
    if (lua.getTop() == 0) return 0;

    _ = lua.getGlobal(VESTI_OUTPUT_STR) catch unreachable;
    lua.rotate(1, 1);
    _ = lua.pushString("\n");
    lua.concat(lua.getTop());
    lua.setGlobal(VESTI_OUTPUT_STR);

    return 0;
}

fn sprintln(lua: *ZigLua) i32 {
    if (lua.getTop() == 0) return 0;

    _ = lua.getGlobal(VESTI_OUTPUT_STR) catch unreachable;
    lua.rotate(1, 1);
    _ = lua.pushString("\n\n");
    lua.concat(lua.getTop());
    lua.setGlobal(VESTI_OUTPUT_STR);

    return 0;
}

fn parse(lua: *ZigLua) i32 {
    if (lua.getTop() == 0) return 0;
    const allocator = lua.allocator();

    _ = lua.pushString("");
    lua.rotate(1, 1);
    lua.concat(lua.getTop());

    const vesti_code = lua.toString(-1) catch {
        const lua_ty = lua.typeOf(-1);
        lua.pop(1);
        var err_msg = ArrayList(u8).initCapacity(allocator, 100) catch @panic("OOM");
        defer err_msg.deinit();
        err_msg.writer().print("expected string, but got {s}", .{luaType2Str(lua_ty)}) catch @panic("OOM");
        err_msg.append(0) catch @panic("OOM");
        _ = lua.pushString(@ptrCast(err_msg.items));
        lua.setGlobal(VESTI_ERROR_STR);
        return 0;
    };

    var diagnostic = diag.Diagnostic{
        .allocator = allocator,
    };
    defer diagnostic.deinit();

    var cwd_dir = std.fs.cwd();
    var parser = Parser.init(
        allocator,
        vesti_code,
        &cwd_dir,
        &diagnostic,
        false, // disallow nested luacode
    ) catch |err| {
        // pop vesti_code
        lua.pop(1);
        var err_msg = ArrayList(u8).initCapacity(allocator, 100) catch @panic("OOM");
        defer err_msg.deinit();
        err_msg.writer().print("parser init faield because of {!}", .{err}) catch @panic("OOM");
        err_msg.append(0) catch @panic("OOM");
        _ = lua.pushString(@ptrCast(err_msg.items));
        lua.setGlobal(VESTI_ERROR_STR);
        return 0;
    };
    defer parser.deinit();

    const ast = parser.parse() catch |err| {
        switch (err) {
            Parser.ParseError.ParseFailed => {
                diagnostic.initMetadata(
                    CowStr.init(.Borrowed, .{@as([]const u8, "<luacode>")}),
                    CowStr.init(.Borrowed, .{@as([]const u8, @ptrCast(vesti_code))}),
                );
                diagnostic.prettyPrint(true) catch @panic("print error on vesti.parse (lua)");
            },
            else => {},
        }

        // pop vesti_code
        lua.pop(1);
        var err_msg = ArrayList(u8).initCapacity(allocator, 100) catch @panic("OOM");
        defer err_msg.deinit();
        err_msg.writer().print("parse failed. error: {!}", .{err}) catch @panic("OOM");
        err_msg.append(0) catch @panic("OOM");
        _ = lua.pushString(@ptrCast(err_msg.items));
        lua.setGlobal(VESTI_ERROR_STR);
        return 0;
    };
    defer {
        for (ast.items) |stmt| stmt.deinit();
        ast.deinit();
    }

    var content = ArrayList(u8).initCapacity(allocator, 256) catch @panic("OOM");
    // content will be assigned into VESTI_OUTPUT_STR global variable
    defer content.deinit();

    const writer = content.writer();
    var codegen = Codegen.init(
        allocator,
        vesti_code,
        ast.items,
        &diagnostic,
    ) catch |err| {
        // pop vesti_code
        lua.pop(1);
        var err_msg = ArrayList(u8).initCapacity(allocator, 100) catch @panic("OOM");
        defer err_msg.deinit();
        err_msg.writer().print("parser init faield because of {!}", .{err}) catch @panic("OOM");
        err_msg.append(0) catch @panic("OOM");
        _ = lua.pushString(@ptrCast(err_msg.items));
        lua.setGlobal(VESTI_ERROR_STR);
        return 0;
    };
    defer codegen.deinit();

    codegen.codegen(writer) catch |err| {
        diagnostic.initMetadata(
            CowStr.init(.Borrowed, .{@as([]const u8, "<luacode>")}),
            CowStr.init(.Borrowed, .{@as([]const u8, @ptrCast(vesti_code))}),
        );
        diagnostic.prettyPrint(true) catch @panic("print error");

        // pop vesti_code
        lua.pop(1);
        var err_msg = ArrayList(u8).initCapacity(allocator, 100) catch @panic("OOM");
        defer err_msg.deinit();
        err_msg.writer().print("parser init faield because of {!}", .{err}) catch @panic("OOM");
        err_msg.append(0) catch @panic("OOM");
        _ = lua.pushString(@ptrCast(err_msg.items));
        lua.setGlobal(VESTI_ERROR_STR);
        return 0;
    };

    // pop vesti_code
    lua.pop(1);
    _ = lua.pushString(content.items);

    return 1;
}

fn luaType2Str(ty: ziglua.LuaType) []const u8 {
    return switch (ty) {
        .none => "none",
        .nil => "nil",
        .boolean => "boolean",
        .light_userdata => "light_userdata",
        .number => "number",
        .string => "string",
        .table => "table",
        .function => "function",
        .userdata => "userdata",
        .thread => "thread",
    };
}
