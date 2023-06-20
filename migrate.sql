create table if not exists ubi_user (
    id varchar(64) not null,
    name varchar(32) not null, 
    ts timestamp not null default CURRENT_TIMESTAMP,
    primary key (id, name)
);
create table if not exists key_word (
    id integer not null primary key autoincrement,
    group_id integer not null,
    regex varchar(128),
    reply varchar(128),
    chance integer not null default 100
);
