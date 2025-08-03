(fill-area
    (fill-area
	(filled-chunk (tile 'concrete))
	(make-area
	    (make-point 0 2)
	    (make-point size-x (- size-y 4)))
	(tile 'asphalt))
    (make-area
        (make-point 0 (- (/ size-y 2) 1))
        (make-point size-x 2))
    (tile 'asphalt-line-horizontal))
