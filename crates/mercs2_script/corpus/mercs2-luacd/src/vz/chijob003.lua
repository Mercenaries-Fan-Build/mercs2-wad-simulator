inherit("MrxTaskJobDestroyType")

function Activated(self)
  MrxTaskJobDestroyType.Activated(self)
  self:_SetLabelFilter("Allied")
  self:_SetHeroOnly(true)
  self:_Go()
end
