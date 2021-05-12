CREATE TYPE some_enum_type AS ENUM ('ga', 'na', 'da');
CREATE TABLE bar (c some_enum_type NOT NULL);
