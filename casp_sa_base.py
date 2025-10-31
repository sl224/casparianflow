from sqlalchemy.orm import DeclarativeBase
from sqlalchemy import inspect

class Base(DeclarativeBase):
    def to_dict(self, exclude_pk=True):

        mapper = inspect(self.__class__)
        
        dict_rep = {}
        for c in mapper.column_attrs:
            if exclude_pk and c.columns[0].primary_key:
                continue
            dict_rep[c.key] = getattr(self, c.key)
            
        return dict_rep