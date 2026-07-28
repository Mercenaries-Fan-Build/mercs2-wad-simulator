inherit("MrxTaskJobDestroyType")
import("MrxVoSequence")
_tTargetCompleteVo = {
  {
    vSequence = "Fiona-In-Mission-Job-Gur01-03",
    nWeight = 3
  },
  {
    vSequence = "Fiona-In-Mission-Job-Gur01-04",
    nWeight = 3
  },
  {
    vSequence = "Fiona-In-Mission-Job-Gur01-05",
    nWeight = 3
  }
}

function Activated(self)
  MrxTaskJobDestroyType.Activated(self)
  self:_SetShortDescription("[GurJob001.Terms.Summary]")
  self:_SetLabelFilter("Billboard")
  self:_SetHeroOnly(true)
  self:_SetTargetCompleteVo(_tTargetCompleteVo)
  self:_Go()
end
