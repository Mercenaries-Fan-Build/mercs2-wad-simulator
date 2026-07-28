inherit("MrxTaskJobDestroyType")

function Activated(self)
  MrxTaskJobDestroyType.Activated(self)
  self:_SetLabelFilter("OC")
  self:_SetHeroOnly(true)
  self:_Go()
end
