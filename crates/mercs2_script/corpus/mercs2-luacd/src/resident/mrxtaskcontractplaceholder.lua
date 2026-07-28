inherit("MrxTaskContract")
import("MrxCinematic")

function Activated(self)
  MrxTaskContract.Activated(self)
  MrxCinematic.PlaceholderSequence({
    {
      sCaption = "This contract is not yet implemented."
    }
  }, self.Complete, {self})
end
