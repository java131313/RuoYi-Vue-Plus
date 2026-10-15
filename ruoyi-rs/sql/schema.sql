-- SQLite schema translated from script/sql/ry_vue_5.X.sql (core system tables only)
PRAGMA journal_mode=WAL;

CREATE TABLE sys_tenant (
    id INTEGER    not null,
    tenant_id TEXT   not null,
    contact_user_name TEXT,
    contact_phone TEXT,
    company_name TEXT,
    license_number TEXT,
    address TEXT,
    intro TEXT,
    domain TEXT,
    remark TEXT,
    package_id INTEGER,
    expire_time TEXT,
    account_count INTEGER           default -1,
    status TEXT       default '0',
    del_flag TEXT       default '0',
    create_dept INTEGER,
    create_by INTEGER,
    create_time TEXT,
    update_by INTEGER,
    update_time TEXT,
    PRIMARY KEY (id)
);

CREATE TABLE sys_dept (
    dept_id INTEGER      not null,
    tenant_id TEXT     default '000000',
    parent_id INTEGER      default 0,
    ancestors TEXT    default '',
    dept_name TEXT     default '',
    dept_category TEXT    default null,
    order_num INTEGER          default 0,
    leader INTEGER      default null,
    phone TEXT     default null,
    email TEXT     default null,
    status TEXT         default '0',
    del_flag TEXT         default '0',
    create_dept INTEGER      default null,
    create_by INTEGER      default null,
    create_time TEXT,
    update_by INTEGER      default null,
    update_time TEXT,
    PRIMARY KEY (dept_id)
);

CREATE TABLE sys_user (
    user_id INTEGER      not null,
    tenant_id TEXT     default '000000',
    dept_id INTEGER      default null,
    user_name TEXT     not null,
    nick_name TEXT     not null,
    user_type TEXT     default 'sys_user',
    email TEXT     default '',
    phonenumber TEXT     default '',
    sex TEXT         default '0',
    avatar INTEGER,
    password TEXT    default '',
    status TEXT         default '0',
    del_flag TEXT         default '0',
    login_ip TEXT    default '',
    login_date TEXT,
    create_dept INTEGER      default null,
    create_by INTEGER      default null,
    create_time TEXT,
    update_by INTEGER      default null,
    update_time TEXT,
    remark TEXT    default null,
    PRIMARY KEY (user_id)
);

CREATE TABLE sys_post (
    post_id INTEGER      not null,
    tenant_id TEXT     default '000000',
    dept_id INTEGER      not null,
    post_code TEXT     not null,
    post_category TEXT    default null,
    post_name TEXT     not null,
    post_sort INTEGER          not null,
    status TEXT         not null,
    create_dept INTEGER      default null,
    create_by INTEGER      default null,
    create_time TEXT,
    update_by INTEGER      default null,
    update_time TEXT,
    remark TEXT    default null,
    PRIMARY KEY (post_id)
);

CREATE TABLE sys_role (
    role_id INTEGER      not null,
    tenant_id TEXT     default '000000',
    role_name TEXT     not null,
    role_key TEXT    not null,
    role_sort INTEGER          not null,
    data_scope TEXT         default '1',
    menu_check_strictly INTEGER      default 1,
    dept_check_strictly INTEGER      default 1,
    status TEXT         not null,
    del_flag TEXT         default '0',
    create_dept INTEGER      default null,
    create_by INTEGER      default null,
    create_time TEXT,
    update_by INTEGER      default null,
    update_time TEXT,
    remark TEXT    default null,
    PRIMARY KEY (role_id)
);

CREATE TABLE sys_menu (
    menu_id INTEGER      not null,
    menu_name TEXT     not null,
    parent_id INTEGER      default 0,
    order_num INTEGER          default 0,
    path TEXT    default '',
    component TEXT    default null,
    query_param TEXT    default null,
    is_frame INTEGER          default 1,
    is_cache INTEGER          default 0,
    menu_type TEXT         default '',
    visible TEXT         default 0,
    status TEXT         default 0,
    perms TEXT    default null,
    icon TEXT    default '#',
    create_dept INTEGER      default null,
    create_by INTEGER      default null,
    create_time TEXT,
    update_by INTEGER      default null,
    update_time TEXT,
    remark TEXT    default '',
    PRIMARY KEY (menu_id)
);

CREATE TABLE sys_user_role (
    user_id INTEGER not null,
    role_id INTEGER not null,
    PRIMARY KEY (user_id, role_id)
);

CREATE TABLE sys_role_menu (
    role_id INTEGER not null,
    menu_id INTEGER not null,
    PRIMARY KEY (role_id, menu_id)
);

CREATE TABLE sys_role_dept (
    role_id INTEGER not null,
    dept_id INTEGER not null,
    PRIMARY KEY (role_id, dept_id)
);

CREATE TABLE sys_user_post (
    user_id INTEGER not null,
    post_id INTEGER not null,
    PRIMARY KEY (user_id, post_id)
);

CREATE TABLE sys_oper_log (
    oper_id INTEGER      not null,
    tenant_id TEXT     default '000000',
    title TEXT     default '',
    business_type INTEGER          default 0,
    method TEXT    default '',
    request_method TEXT     default '',
    operator_type INTEGER          default 0,
    oper_name TEXT     default '',
    dept_name TEXT     default '',
    oper_url TEXT    default '',
    oper_ip TEXT    default '',
    oper_location TEXT    default '',
    oper_param TEXT   default '',
    json_result TEXT   default '',
    status INTEGER          default 0,
    error_msg TEXT   default '',
    oper_time TEXT,
    cost_time INTEGER      default 0,
    PRIMARY KEY (oper_id)
);

CREATE TABLE sys_dict_type (
    dict_id INTEGER      not null,
    tenant_id TEXT     default '000000',
    dict_name TEXT    default '',
    dict_type TEXT    default '',
    create_dept INTEGER      default null,
    create_by INTEGER      default null,
    create_time TEXT,
    update_by INTEGER      default null,
    update_time TEXT,
    remark TEXT    default null,
    PRIMARY KEY (dict_id),
    UNIQUE (tenant_id, dict_type)
);

CREATE TABLE sys_dict_data (
    dict_code INTEGER      not null,
    tenant_id TEXT     default '000000',
    dict_sort INTEGER          default 0,
    dict_label TEXT    default '',
    dict_value TEXT    default '',
    dict_type TEXT    default '',
    css_class TEXT    default null,
    list_class TEXT    default null,
    is_default TEXT         default 'N',
    create_dept INTEGER      default null,
    create_by INTEGER      default null,
    create_time TEXT,
    update_by INTEGER      default null,
    update_time TEXT,
    remark TEXT    default null,
    PRIMARY KEY (dict_code)
);

CREATE TABLE sys_config (
    config_id INTEGER      not null,
    tenant_id TEXT     default '000000',
    config_name TEXT    default '',
    config_key TEXT    default '',
    config_value TEXT    default '',
    config_type TEXT         default 'N',
    create_dept INTEGER      default null,
    create_by INTEGER      default null,
    create_time TEXT,
    update_by INTEGER      default null,
    update_time TEXT,
    remark TEXT    default null,
    PRIMARY KEY (config_id)
);

CREATE TABLE sys_logininfor (
    info_id INTEGER     not null,
    tenant_id TEXT    default '000000',
    user_name TEXT    default '',
    client_key TEXT    default '',
    device_type TEXT    default '',
    ipaddr TEXT   default '',
    login_location TEXT   default '',
    browser TEXT    default '',
    os TEXT    default '',
    status TEXT        default '0',
    msg TEXT   default '',
    login_time TEXT,
    PRIMARY KEY (info_id)
);

CREATE TABLE sys_notice (
    notice_id INTEGER      not null,
    tenant_id TEXT     default '000000',
    notice_title TEXT     not null,
    notice_type TEXT         not null,
    notice_content TEXT        default null,
    status TEXT         default '0',
    create_dept INTEGER      default null,
    create_by INTEGER      default null,
    create_time TEXT,
    update_by INTEGER      default null,
    update_time TEXT,
    remark TEXT    default null,
    PRIMARY KEY (notice_id)
);

CREATE TABLE sys_client (
    id INTEGER    not null,
    client_id TEXT   default null,
    client_key TEXT   default null,
    client_secret TEXT  default null,
    grant_type TEXT  default null,
    device_type TEXT   default null,
    active_timeout INTEGER       default 1800,
    timeout INTEGER       default 604800,
    status TEXT       default '0',
    del_flag TEXT       default '0',
    create_dept INTEGER    default null,
    create_by INTEGER    default null,
    create_time TEXT      default null,
    update_by INTEGER    default null,
    update_time TEXT      default null,
    PRIMARY KEY (id)
);
